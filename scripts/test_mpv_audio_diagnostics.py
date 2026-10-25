"""Run the actual embedded Lua diagnostics with a simulated mpv API (requires lupa)."""
from pathlib import Path
import unittest

from lupa import LuaRuntime

SOURCE = (Path(__file__).resolve().parents[1] / "src/main.rs").read_text(encoding="utf-8")
BLOCK = SOURCE.split("-- Read-only audio diagnostics.", 1)[1].split(
    "local function observe_property(name)", 1
)[0]
BLOCK = "-- Read-only audio diagnostics." + BLOCK


class AudioDiagnosticsTest(unittest.TestCase):
    def setUp(self):
        self.lua = LuaRuntime()
        self.lua.execute('''
            logs, timers, events, observers, commands = {}, {}, {}, {}, {}
            aborted = 0
            function log_line(text) table.insert(logs, text) end
            function native_to_text(value) return tostring(value) end
            mp = {}
            function mp.add_timeout(delay, callback)
                local t = {delay=delay, callback=callback, killed=false}
                function t:kill() self.killed=true end
                table.insert(timers, t)
                return t
            end
            function mp.command_native_async(command, callback)
                table.insert(commands, command)
                completion = callback
                return 17
            end
            function mp.abort_async_command(id) assert(id==17); aborted=aborted+1 end
            function mp.register_event(name, callback) events[name]=callback end
            function mp.observe_property(name, kind, callback) observers[name]=callback end
            function mp.enable_messages(level) assert(level=='v') end
        ''')
        self.lua.execute(BLOCK)

    def test_entire_embedded_script_parses(self):
        script = SOURCE.split('script.push_str(r#"local recording = false', 1)[1]
        script = "local recording = false" + script.split('"#);', 1)[0]
        self.lua.execute("assert(load(...))", script)

    def test_probe_is_async_debounced_and_logs_system_default(self):
        self.lua.execute('''
            events['file-loaded']()
            events['audio-reconfig']()
            observers['audio-device-list']('audio-device-list', 'hearing aids')
            assert(#timers==1 and #commands==0)
            timers[1].callback()
            assert(#commands==1 and commands[1].name=='subprocess')
            assert(commands[1].args[1]=='/usr/sbin/system_profiler')
            assert(commands[1].args[2]=='SPAudioDataType')
            assert(commands[1].capture_stdout and commands[1].playback_only==false)
            completion(true, {status=0, stdout='{"_name":"Hearing aids","coreaudio_default_audio_output_device":"spaudio_yes"}'}, nil)
            assert(timers[2].killed)
            assert(logs[#logs]:find('Hearing aids'))
            assert(logs[#logs]:find('coreaudio_default_audio_output_device'))
            events['audio-reconfig']()
            assert(#timers==3)
        ''')

    def test_failure_does_not_prevent_next_probe(self):
        self.lua.execute('''
            events['file-loaded'](); timers[1].callback()
            completion(false, nil, 'failed')
            assert(logs[#logs]:find('audio.system_devices.failed'))
            events['audio-reconfig'](); timers[3].callback()
            assert(#commands==2)
        ''')

    def test_timeout_aborts_only_diagnostic_process(self):
        self.lua.execute('''
            events['file-loaded'](); timers[1].callback(); timers[2].callback()
            assert(aborted==1)
            completion(false, nil, 'aborted')
            events['audio-reconfig']()
            assert(#timers==3)
        ''')

    def test_shutdown_cancels_pending_and_running_probes(self):
        self.lua.execute('''
            events['file-loaded'](); events.shutdown(); timers[1].callback()
            assert(#commands==0)
        ''')
        self.setUp()
        self.lua.execute('''
            events['file-loaded'](); timers[1].callback(); events.shutdown()
            assert(aborted==1 and timers[2].killed)
            completion(false, nil, 'aborted')
            assert(#logs==0)
        ''')

    def test_actual_device_messages_are_recorded_not_unrelated_messages(self):
        self.lua.execute('''
            events['log-message']({prefix='ao/coreaudio',text='selected audio output device: Hearing aids\n'})
            events['log-message']({prefix='cplayer',text='AO: [coreaudio] 48000Hz stereo'})
            events['log-message']({prefix='vo/gpu',text='unrelated'})
            assert(#logs==2)
            assert(logs[1]:find('Hearing aids'))
        '''.replace('aids\n', 'aids\\n'))


if __name__ == "__main__":
    unittest.main()
