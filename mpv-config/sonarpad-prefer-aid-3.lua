local preferred_aid = 3

mp.register_event("file-loaded", function()
    local tracks = mp.get_property_native("track-list", {})
    for _, track in ipairs(tracks) do
        if track.type == "audio" and track.id == preferred_aid then
            mp.set_property_number("aid", preferred_aid)
            return
        end
    end
    mp.set_property("aid", "auto")
end)
