-- Shared SonarTube/SonarMusic recovery, in the existing mpv process.
local mp = require 'mp'
local opts = {url = ''}
require('mp.options').read_options(opts, 'sonarpad-youtube-recovery')
if opts.url == '' then return end
local hls = 'bestvideo[protocol^=m3u8][height<=360]+bestaudio[protocol^=m3u8]/best[protocol^=m3u8]'
local ready, reloading, armed = false, false, false
local position, last_position = 0, nil
local progress_at, paused_at = mp.get_time(), nil
local attempts, error_attempts = 0, 0
local media_error = false

local function finished()
    local duration = mp.get_property_number('duration', 0)
    return mp.get_property_native('eof-reached', false) or
        (duration > 0 and position >= duration - 1)
end

local refresh
refresh = function(reason)
    if not armed or reloading or attempts >= 2 then return false end
    if reason ~= 'media-error' and reason ~= 'timeout' and finished() then return false end
    position = math.max(0, mp.get_property_number('time-pos', position))
    attempts = attempts + 1
    reloading, ready = true, false
    mp.msg.info('Sonarpad YouTube recovery: ' .. reason .. ' attempt=' .. attempts)
    local raw = mp.get_property_native('options/ytdl-raw-options', {})
    raw['extractor-args'] = nil
    mp.set_property_native('options/ytdl-raw-options', raw)
    mp.set_property('options/ytdl', 'yes')
    mp.set_property('options/ytdl-format', hls)
    -- Canonical URL resolves new signed media URLs and preserves the current position.
    mp.command_native({'loadfile', 'ytdl://' .. opts.url, 'replace', -1,
        {start = tostring(position), pause = 'no'}})
    local attempt = attempts
    mp.add_timeout(60, function()
        if armed and reloading and attempts == attempt and attempts < 2 then
            reloading = false
            refresh('timeout')
        end
    end)
    return true
end

mp.register_event('shutdown', function() armed = false; ready = false end)
mp.register_event('start-file', function()
    ready = false
    media_error = false
    last_position = nil
    progress_at = mp.get_time()
end)
mp.register_event('playback-restart', function()
    ready, reloading = true, false
    progress_at = mp.get_time()

end)
-- The host transfers ownership only after its startup checks succeed.
mp.register_script_message('sonarpad-youtube-recovery-ready', function() armed = true end)
mp.enable_messages('v')
mp.register_event('log-message', function(event)
    local text = (event.text or ''):lower()
    if text:find('got eof with no data before it', 1, true) or
       text:find('http error 403', 1, true) or text:find('http error 500', 1, true) or
       text:find('requested format is not available', 1, true) or
       text:find('no video formats found', 1, true) then
        media_error = true
    end
end)
mp.register_event('end-file', function(event)
    ready, reloading = false, false
    if (event.reason == 'error' or event.reason == 'eof') and media_error and error_attempts < 1 then
        error_attempts = error_attempts + 1
        refresh('media-error')
    end
end)
mp.observe_property('pause', 'bool', function(_, paused)
    if paused then
        paused_at = mp.get_time()
    elseif paused_at then
        local elapsed = mp.get_time() - paused_at
        paused_at = nil
        progress_at = mp.get_time()
        if elapsed >= 600 then refresh('long-pause') end
    end
end)
mp.add_periodic_timer(3, function()
    if not ready or reloading then return end
    if mp.get_property_native('pause', false) then
        progress_at = mp.get_time()
        return
    end
    local current = mp.get_property_number('time-pos')
    if not current then progress_at = mp.get_time(); return end
    position = math.max(0, current)
    if last_position == nil or math.abs(current - last_position) >= 0.5 then
        progress_at = mp.get_time()
    elseif mp.get_time() - progress_at >= 20 then
        refresh('stalled')
    end
    last_position = current
end)
