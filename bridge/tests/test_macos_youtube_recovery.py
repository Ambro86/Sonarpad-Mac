"""Execute the shared mpv recovery script with a deterministic player clock."""
import unittest
from pathlib import Path
try:
    from lupa import LuaRuntime
except ImportError:
    LuaRuntime = None

@unittest.skipIf(LuaRuntime is None, "Lua runtime (lupa) unavailable")
class YoutubeRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.lua = LuaRuntime(unpack_returned_tuples=True)
        self.lua.execute("""
        now = 0; events = {}; observers = {}; timers = {}; commands = {}; messages = {}
        properties = {['time-pos']=12, duration=245, pause=false, ['eof-reached']=false,
            ['options/ytdl-raw-options']={['extractor-args']='youtube:player_client=android', ['js-runtimes']='deno:/bundled/deno'}}
        mp = {
            get_time=function() return now end,
            get_property_number=function(k,d) if properties[k] == nil then return d end return properties[k] end,
            get_property_native=function(k,d) if properties[k] == nil then return d end return properties[k] end,
            set_property=function(k,v) properties[k]=v end,
            set_property_native=function(k,v) properties[k]=v end,
            command_native=function(c) table.insert(commands,c) end,
            register_event=function(k,f) events[k]=f end,
            register_script_message=function(k,f) messages[k]=f end,
            observe_property=function(k,t,f) observers[k]=f end,
            enable_messages=function() end,
            add_timeout=function(t,f) table.insert(timers,{at=now+t,fn=f}) end,
            add_periodic_timer=function(t,f) tick=f end,
            msg={info=function() end}
        }
        package.preload['mp']=function() return mp end
        package.preload['mp.options']=function() return {read_options=function(o) o.url='https://www.youtube.com/watch?v=lvaC8GmvTrY' end} end
        function advance(t)
            now=now+t
            local old=timers; timers={}
            for _,v in ipairs(old) do if v.at <= now then v.fn() else table.insert(timers,v) end end
        end
        function started()
            events['start-file'](); events['playback-restart'](); messages['sonarpad-youtube-recovery-ready'](); advance(1); tick()
        end
        """)
        script = Path(__file__).resolve().parents[2] / 'src/youtube_recovery.lua'
        self.lua.execute(script.read_text(encoding='utf-8'))

    def test_stall_refresh_preserves_position_and_runtime(self):
        self.lua.execute("started(); advance(21); tick()")
        self.assertEqual(self.lua.eval('#commands'), 1)
        self.assertEqual(self.lua.eval('commands[1][5].start'), '12')
        self.assertIsNone(self.lua.eval("properties['options/ytdl-raw-options']['extractor-args']"))
        self.assertEqual(self.lua.eval("properties['options/ytdl-raw-options']['js-runtimes']"), 'deno:/bundled/deno')

    def test_pause_and_normal_end_never_trigger_stall(self):
        self.lua.execute("started(); properties.pause=true; advance(30); tick(); properties.pause=false; properties['eof-reached']=true; advance(30); tick()")
        self.assertEqual(self.lua.eval('#commands'), 0)

    def test_long_pause_refreshes_once_on_resume(self):
        self.lua.execute("started(); observers.pause('',true); advance(601); observers.pause('',false)")
        self.assertEqual(self.lua.eval('#commands'), 1)

    def test_timeout_retry_is_bounded(self):
        self.lua.execute("started(); advance(21); tick(); advance(61); advance(61)")
        self.assertEqual(self.lua.eval('#commands'), 2)

    def test_early_failure_is_left_to_host(self):
        self.lua.execute("events['log-message']({text='Got EOF with no data before it'}); events['end-file']({reason='eof'})")
        self.assertEqual(self.lua.eval('#commands'), 0)

    def test_late_empty_audio_recovers_but_user_stop_does_not(self):
        self.lua.execute("started(); events['log-message']({text='Got EOF with no data before it'}); events['end-file']({reason='stop'})")
        self.assertEqual(self.lua.eval('#commands'), 0)
        self.lua.execute("events['end-file']({reason='eof'})")
        self.assertEqual(self.lua.eval('#commands'), 1)

    def test_shutdown_cancels_pending_retry(self):
        self.lua.execute("started(); advance(21); tick(); events.shutdown(); advance(61)")
        self.assertEqual(self.lua.eval('#commands'), 1)

if __name__ == '__main__':
    unittest.main()
