// Audio-only capture helper. Built into Sonarpad.app; never downloaded at runtime.
import Foundation
import AVFoundation
import ScreenCaptureKit
import CoreMedia

func emit(_ value: [String: Any]) {
    if let data = try? JSONSerialization.data(withJSONObject: value) {
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([10]))
    }
}
func clockNow() -> Double { CMTimeGetSeconds(CMClockGetTime(CMClockGetHostTimeClock())) }

// Host-clock timeline shared by microphone and system audio. Pauses consume no output time.
struct RecordingTimeline {
    var origin: Double?
    var pausedAt: Double?
    var pausedTotal = 0.0
    var resumeBoundary = 0.0
    mutating func begin(_ now: Double) { origin = now; resumeBoundary = now }
    mutating func pause(_ now: Double) { if pausedAt == nil { pausedAt = now } }
    mutating func resume(_ now: Double) {
        if let paused = pausedAt { pausedTotal += now - paused; pausedAt = nil; resumeBoundary = now }
    }
    func duration(_ now: Double) -> Double {
        guard let origin = origin else { return 0 }
        return max(0, (pausedAt ?? now) - origin - pausedTotal)
    }
    func position(_ timestamp: Double) -> Double? {
        guard let origin = origin, pausedAt == nil, timestamp.isFinite, timestamp >= resumeBoundary else { return nil }
        return timestamp - origin - pausedTotal
    }
}

func selfTest() {
    var t = RecordingTimeline()
    assert(t.position(1) == nil)
    t.begin(100); assert(t.position(99.9) == nil); assert(t.position(101) == 1)
    t.pause(102); t.pause(103); assert(t.duration(105) == 2); assert(t.position(103) == nil)
    t.resume(112); assert(t.position(111) == nil); assert(t.position(113) == 3)
    t.resume(114); assert(t.duration(114) == 4)
    t.pause(115); assert(t.duration(200) == 5)
    t.resume(215); assert(t.position(216) == 6)
    assert(t.position(.nan) == nil)
    let inputFormat = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 1)!
    let outputFormat = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
    let converter = AVAudioConverter(from: inputFormat, to: outputFormat)!
    let input = AVAudioPCMBuffer(pcmFormat: inputFormat, frameCapacity: 4410)!
    input.frameLength = 4410
    for i in 0..<4410 { input.floatChannelData![0][i] = Float(sin(Double(i) * 440 * 2 * .pi / 44100) * 0.2) }
    let output = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: 4864)!
    var supplied = false
    var error: NSError?
    let result = converter.convert(to: output, error: &error) { _, status in
        if supplied { status.pointee = .endOfStream; return nil }
        supplied = true; status.pointee = .haveData; return input
    }
    assert(result != .error && error == nil && output.frameLength > 4500)
    assert((0..<Int(output.frameLength)).contains { abs(output.floatChannelData![0][$0]) > 0.05 })
    emit(["self_test": "passed", "cases": ["startup", "pause", "resume", "multiple pauses", "stale buffers", "mono 44100 to stereo 48000"]])
}

// All timeline changes, converters and stdout writes run on the same serial queue.
final class Recorder: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
    let queue = DispatchQueue(label: "org.sonarpad.podcast.capture")
    let session = AVCaptureSession()
    var systemCapture: AnyObject?
    var observers: [NSObjectProtocol] = []
    var converters: [String: AVAudioConverter] = [:]
    var timeline = RecordingTimeline()
    var stopping = false
    var timer: DispatchSourceTimer?
    let destination = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 48000, channels: 2, interleaved: false)!

    func duration(_ now: Double = clockNow()) -> Double {
        timeline.duration(now)
    }
    func fail(_ code: String, _ detail: String) {
        emit(["event": "error", "code": code, "detail": detail])
        exit(1)
    }
    func consume(_ sample: CMSampleBuffer, source: String) {
        guard !stopping, CMSampleBufferDataIsReady(sample),
              let desc = CMSampleBufferGetFormatDescription(sample) else { return }
        let format = AVAudioFormat(cmAudioFormatDescription: desc)
        var presentation = CMSampleBufferGetPresentationTimeStamp(sample)
        if source == "microphone", let clock = session.masterClock {
            presentation = CMSyncConvertTime(presentation, from: clock, to: CMClockGetHostTimeClock())
        }
        let timestamp = CMTimeGetSeconds(presentation)
        guard let time = timeline.position(timestamp) else { return }
        let count = CMSampleBufferGetNumSamples(sample)
        guard count > 0, let input = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(count)) else { return }
        input.frameLength = AVAudioFrameCount(count)
        let result = CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(count), into: input.mutableAudioBufferList)
        guard result == noErr else { fail("capture", "PCM buffer error \(result)"); return }
        var converter = converters[source]
        if converter == nil || converter!.inputFormat != format {
            converter = AVAudioConverter(from: format, to: destination)
            converters[source] = converter
        }
        guard let converter = converter,
              let output = AVAudioPCMBuffer(pcmFormat: destination, frameCapacity: AVAudioFrameCount(ceil(Double(count) * 48000 / format.sampleRate)) + 64) else {
            fail("capture", "Cannot convert captured audio"); return
        }
        var supplied = false
        var error: NSError?
        let status = converter.convert(to: output, error: &error) { _, state in
            if supplied { state.pointee = .noDataNow; return nil }
            supplied = true; state.pointee = .haveData; return input
        }
        guard status != .error, error == nil else { fail("capture", error?.localizedDescription ?? "Audio conversion failed"); return }
        guard let channels = output.floatChannelData, output.frameLength > 0 else { return }
        var packed = [Float](repeating: 0, count: Int(output.frameLength) * 2)
        for frame in 0..<Int(output.frameLength) {
            packed[frame * 2] = channels[0][frame]
            packed[frame * 2 + 1] = channels[1][frame]
        }
        // Reject stale startup packets. Both sources use the host clock, not callback arrival time.
        guard time >= 0 else { return }
        let data = packed.withUnsafeBytes { Data($0) }
        emit(["event": "audio", "source": source, "time": time, "data": data.base64EncodedString()])
    }
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        consume(sampleBuffer, source: "microphone")
    }
    func startMicrophone(_ id: String) throws {
        let device: AVCaptureDevice?
        if id == "default" { device = AVCaptureDevice.default(for: .audio) }
        else { device = AVCaptureDevice.devices(for: .audio).first { $0.uniqueID == id } }
        guard let device = device else { throw NSError(domain: "No microphone available", code: 1) }
        let input = try AVCaptureDeviceInput(device: device)
        let output = AVCaptureAudioDataOutput()
        output.setSampleBufferDelegate(self, queue: queue)
        guard session.canAddInput(input), session.canAddOutput(output) else { throw NSError(domain: "Microphone unavailable", code: 2) }
        session.addInput(input); session.addOutput(output)
        for name in [AVCaptureSession.runtimeErrorNotification, AVCaptureSession.wasInterruptedNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: session, queue: nil) { notification in
                self.queue.async {
                    if !self.stopping {
                        let error = notification.userInfo?[AVCaptureSessionErrorKey] as? NSError
                        self.fail("microphone", error?.localizedDescription ?? "Microphone capture interrupted")
                    }
                }
            })
        }
        session.startRunning()
        guard session.isRunning else { throw NSError(domain: "Microphone did not start", code: 4) }
    }
    func ready() {
        queue.async {
            self.timeline.begin(clockNow())
            emit(["event": "ready"])
            let timer = DispatchSource.makeTimerSource(queue: self.queue)
            timer.schedule(deadline: .now(), repeating: 0.25)
            timer.setEventHandler { emit(["event": "time", "seconds": self.duration()]) }
            timer.resume(); self.timer = timer
        }
    }
    func command(_ command: String) {
        queue.async {
            guard self.timeline.origin != nil, !self.stopping else { return }
            let now = clockNow()
            switch command {
            case "pause":
                self.timeline.pause(now)
                emit(["event": "paused", "seconds": self.duration()])
            case "resume":
                self.timeline.resume(now)
                self.converters.removeAll()
                emit(["event": "resumed", "seconds": self.duration()])
            case "stop":
                self.stopping = true; self.timer?.cancel()
                // All earlier audio writes completed on this serial queue. Exiting closes capture devices.
                emit(["event": "stopped", "seconds": self.duration(now)])
                exit(0)
            default: break
            }
        }
    }
}

@available(macOS 13.0, *)
final class SystemCapture: NSObject, SCStreamOutput, SCStreamDelegate {
    let recorder: Recorder
    var stream: SCStream?
    init(_ recorder: Recorder) { self.recorder = recorder }
    func start(_ completion: @escaping (Error?) -> Void) {
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: false) { content, error in
            guard let content = content, let display = content.displays.first else {
                completion(error ?? NSError(domain: "System audio permission or display unavailable", code: 3)); return
            }
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = false
            config.sampleRate = 48000; config.channelCount = 2
            // No screen output is registered or saved. Minimize the unused video stream.
            config.width = 2; config.height = 2
            config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
            config.showsCursor = false
            let stream = SCStream(filter: filter, configuration: config, delegate: self)
            self.stream = stream
            do { try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: self.recorder.queue) }
            catch { completion(error); return }
            stream.startCapture(completionHandler: completion)
        }
    }
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        if type == .audio { recorder.consume(sampleBuffer, source: "system") }
    }
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        recorder.queue.async { self.recorder.fail("capture", error.localizedDescription) }
    }
}

let args = CommandLine.arguments
if args.contains("--self-test") { selfTest(); exit(0) }
if args.contains("--devices") {
    let devices = AVCaptureDevice.devices(for: .audio).map { ["id": $0.uniqueID, "name": $0.localizedName] }
    var system = false
    if #available(macOS 13.0, *) { system = true }
    emit(["devices": devices, "system": system]); exit(0)
}
let recorder = Recorder()
let system = args.contains("--system")
let micIndex = args.firstIndex(of: "--microphone")
let mic = micIndex.flatMap { $0 + 1 < args.count ? args[$0 + 1] : nil }
if !system && mic == nil { recorder.fail("source", "No recording source selected") }
func begin() {
    DispatchQueue.global().async {
        do { if let mic = mic { try recorder.startMicrophone(mic) } }
        catch { recorder.fail("microphone", error.localizedDescription); return }
        if system {
            if #available(macOS 13.0, *) {
                let capture = SystemCapture(recorder); recorder.systemCapture = capture
                capture.start { error in
                    if let error = error { recorder.fail("system", error.localizedDescription) }
                    else { recorder.ready() }
                }
            } else { recorder.fail("unsupported", "System audio requires macOS 13 or later") }
        } else { recorder.ready() }
    }
}
if mic != nil {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: begin()
    case .notDetermined:
        AVCaptureDevice.requestAccess(for: .audio) { allowed in
            if allowed { begin() } else { recorder.fail("microphone_permission", "Microphone access denied") }
        }
    default: recorder.fail("microphone_permission", "Microphone access denied")
    }
} else { begin() }
DispatchQueue.global().async {
    while let line = readLine() { recorder.command(line) }
    // Parent went away: never keep recording in an orphan process.
    exit(0)
}
RunLoop.main.run()
