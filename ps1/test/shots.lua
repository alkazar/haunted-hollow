-- Takes screenshots at the vblanks listed in HH_SHOTS (comma separated) into
-- HH_SHOT_DIR, holds pad buttons as listed in HH_PRESS ("frame:BUTTON:frames,...",
-- e.g. "600:CROSS:4"), then quits after the last screenshot.
local dir = os.getenv('HH_SHOT_DIR') or '.'
local list = {}
for n in string.gmatch(os.getenv('HH_SHOTS') or '600', '%d+') do list[#list + 1] = tonumber(n) end
local presses = {}
for f, b, n in string.gmatch(os.getenv('HH_PRESS') or '', '(%d+):(%u+):(%d+)') do
	presses[#presses + 1] = { at = tonumber(f), button = b, len = tonumber(n) }
end
local last = list[#list]
local frames = 0
shot_listener = PCSX.Events.createEventListener('GPU::Vsync', function()
	frames = frames + 1
	local pad = PCSX.SIO0.slots[1].pads[1]
	for _, p in ipairs(presses) do
		local b = PCSX.CONSTS.PAD.BUTTON[p.button]
		if frames == p.at then pad.setOverride(b) end
		if frames == p.at + p.len then pad.clearOverride(b) end
	end
	for _, f in ipairs(list) do
		if frames == f then
			local ss = PCSX.GPU.takeScreenShot()
			local file = Support.File.open(dir .. '/shot_' .. f .. '.raw', 'TRUNCATE')
			file:write(ss.data)
			file:close()
			print(string.format('shot %d %dx%d', f, ss.width, ss.height))
		end
	end
	if frames > last + 2 then PCSX.quit() end
end)
