# Haunted Hollow for the PlayStation, built with PSn00bSDK. The data files come
# from the PC game: `haunted_hollow --export-ps1 <dir>` (see build.sh), passed in
# as HH_DATA.

cmake_minimum_required(VERSION 3.21)

project(
	haunted-hollow-ps1
	LANGUAGES    C ASM
	VERSION      1.0.0
	DESCRIPTION  "Haunted Hollow for the PlayStation"
)

if(NOT HH_DATA)
	message(FATAL_ERROR "Set HH_DATA to the directory written by haunted_hollow --export-ps1")
endif()

# The game itself is shared with the Wii version (../shared); this directory has
# the PS1's graphics, sound, controllers and CD streaming.
psn00bsdk_add_executable(
	hollow GPREL
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
target_compile_options(hollow PRIVATE -O2 -Wall -Wno-misleading-indentation -fno-strict-aliasing)
# -DHH_BOTS=N puts N computer players on the first controllers (attract mode, tests).
set(HH_BOTS 0 CACHE STRING "Computer players")
target_compile_definitions(hollow PRIVATE HH_BOTS=${HH_BOTS})

# -DHH_DEBUG=ON shows frame rate and timings in the corner.
option(HH_DEBUG "Show frame rate and timings" OFF)
if(HH_DEBUG)
	target_compile_definitions(hollow PRIVATE HH_DEBUG)
endif()

# For automated tests: -DHH_TEST_BOSS=ON skips the ride and gives the boss 3 hits.
option(HH_TEST_BOSS "Skip the ride and make the boss quick (for testing)" OFF)
if(HH_TEST_BOSS)
	target_compile_definitions(hollow PRIVATE HH_TEST_BOSS)
endif()

foreach(name pvs track models spawns light sky scenery sound)
	psn00bsdk_target_incbin(hollow PRIVATE ${name}_bin ${HH_DATA}/${name}.bin)
endforeach()

configure_file(iso.xml ${CMAKE_CURRENT_BINARY_DIR}/iso.xml)
psn00bsdk_add_cd_image(
	iso
	haunted-hollow
	${CMAKE_CURRENT_BINARY_DIR}/iso.xml
	DEPENDS hollow system.cnf ${HH_DATA}/WORLD.BIN ${HH_DATA}/ATLAS.BIN ${HH_DATA}/SOUND.BIN
)
