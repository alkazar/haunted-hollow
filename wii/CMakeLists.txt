# Haunted Hollow for the Wii, built with devkitPPC and libogc (see build.sh, which
# runs this in devkitPro's container). The game itself is shared with the PS1
# version (../shared). The data files come from the PC game:
# `haunted_hollow --export-wii <dir>`, passed in as HH_DATA.

cmake_minimum_required(VERSION 3.13)
project(haunted-hollow-wii LANGUAGES C ASM)

if(NOT HH_DATA)
	message(FATAL_ERROR "Set HH_DATA to the directory written by haunted_hollow --export-wii")
endif()

add_executable(
	hollow
	src/main.c
	src/gfx.c
	src/stream.c
	src/audio.c
	src/input.c
	../shared/game.c
	../shared/cam.c
	../shared/util.c
)
target_include_directories(hollow PRIVATE src ../shared)
target_compile_options(hollow PRIVATE -O2 -Wall -Wno-misleading-indentation)
target_link_libraries(hollow PRIVATE wiiuse bte asnd m)

# -DHH_BOTS=N puts N computer players on the first controllers (attract mode, tests).
set(HH_BOTS 0 CACHE STRING "Computer players")
target_compile_definitions(hollow PRIVATE HH_BOTS=${HH_BOTS})

# -DHH_DEBUG=ON shows frame rate and timings in the corner.
option(HH_DEBUG "Show frame rate and timings" OFF)
if(HH_DEBUG)
	target_compile_definitions(hollow PRIVATE HH_DEBUG)
endif()

# The data files, linked in as <symbol> (32-byte aligned) and <symbol>_size.
foreach(entry pvs.bin:pvs_bin track.bin:track_bin models.bin:models_bin spawns.bin:spawns_bin
		light.bin:light_bin sky.bin:sky_bin scenery.bin:scenery_bin WORLD.BIN:world_bin
		ATLAS.BIN:atlas_bin SOUND.BIN:wii_sound_bin)
	string(REPLACE ":" ";" parts ${entry})
	list(GET parts 0 file)
	list(GET parts 1 sym)
	set(asm ${CMAKE_CURRENT_BINARY_DIR}/embed_${sym}.s)
	file(WRITE ${asm}
		".section .rodata.${sym}, \"a\"\n.balign 32\n.global ${sym}\n${sym}:\n"
		".incbin \"${HH_DATA}/${file}\"\n${sym}_end:\n"
		".balign 4\n.global ${sym}_size\n${sym}_size:\n.int ${sym}_end - ${sym}\n")
	target_sources(hollow PRIVATE ${asm})
	set_source_files_properties(${asm} PROPERTIES OBJECT_DEPENDS ${HH_DATA}/${file})
endforeach()

ogc_create_dol(hollow)
