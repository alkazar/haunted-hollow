//! VR mode (OpenXR, `--vr`): one rider in a headset. The game runs exactly as it does on a TV,
//! with the same camera, the same targets and the same 2D HUD. The headset view sits on top:
//! - The game camera is the cart. The rider's head can move and look around inside it to
//!   admire the scenery, but targets still appear in front of the (forward-facing) camera.
//! - The HUD (crosshair, scores, banners) is drawn on a virtual screen that exactly fills the
//!   game camera's view, `HUD_DIST` ahead, so seen from the cart it lines up with the world.
//! - The controller is a laser pointer. Its beam ends on a sphere `HUD_DIST` around the rider,
//!   where the crosshair floats, facing them; the screen point behind it is the game's aim.

use crate::game::{Game, SourceId, SourceInput, FOV_Y, PLAYER_COLORS};
use crate::{atlas, audio, hud, mesh, step_game, vk as gfx};
use ash::vk::{self, Handle};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3, Vec4};
use openxr as xr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const VIEW_TYPE: xr::ViewConfigurationType = xr::ViewConfigurationType::PRIMARY_STEREO;
/// The game's screen size in VR. Nothing is shown at this resolution: it is just the
/// coordinate system of the HUD and of aiming (16:9 like a TV, so targets are placed the same).
const SCREEN: Vec2 = Vec2::new(1280.0, 720.0);
/// How far ahead of the camera the virtual screen floats, and the radius of the sphere the
/// crosshair moves on (targets appear 6.5-15 m away).
const HUD_DIST: f32 = 9.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 800.0;

static QUIT: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigint(_: libc::c_int) {
    QUIT.store(true, Ordering::Relaxed);
}

/// The cart's camera this frame: an orthonormal frame the rider's head lives in.
struct Cart {
    eye: Vec3,
    dir: Vec3,
    right: Vec3,
    up: Vec3,
    rot: Quat,
}

impl Cart {
    fn new(eye: Vec3, dir: Vec3, up: Vec3) -> Cart {
        let right = dir.cross(up).normalize();
        let up = right.cross(dir);
        // Tracking space: +x right, +y up, -z forward.
        let rot = Quat::from_mat3(&Mat3::from_cols(right, up, -dir));
        Cart { eye, dir, right, up, rot }
    }

    /// A pose in tracking space (relative to where the rider's head started) -> world.
    fn to_world(&self, pose: &xr::Posef) -> (Vec3, Quat) {
        let p = Vec3::new(pose.position.x, pose.position.y, pose.position.z);
        let o = pose.orientation;
        (self.eye + self.rot * p, self.rot * Quat::from_xyzw(o.x, o.y, o.z, o.w))
    }

    /// Half the virtual screen's size, as tangents of the game camera's half angles.
    fn half_tan(&self) -> Vec2 {
        let ty = (FOV_Y.to_radians() * 0.5).tan();
        Vec2::new(ty * SCREEN.x / SCREEN.y, ty)
    }

    /// Maps HUD pixels (x, y, 0, 1) onto the virtual screen in the world.
    fn hud_to_world(&self) -> Mat4 {
        let h = self.half_tan() * HUD_DIST;
        let corner = self.eye + self.dir * HUD_DIST - self.right * h.x + self.up * h.y;
        Mat4::from_cols(
            (self.right * (2.0 * h.x / SCREEN.x)).extend(0.0),
            (self.up * (-2.0 * h.y / SCREEN.y)).extend(0.0),
            self.dir.extend(0.0),
            corner.extend(1.0),
        )
    }

    /// Where a ray from inside the aiming sphere (the controller) comes out of it.
    fn sphere_hit(&self, from: Vec3, dir: Vec3) -> Vec3 {
        let o = from - self.eye;
        let b = o.dot(dir);
        let t = -b + (b * b - o.length_squared() + HUD_DIST * HUD_DIST).max(0.0).sqrt();
        from + dir * t
    }

    /// The HUD pixel the camera sees a world point at, if it is in front of the camera
    /// (not more than about 87 degrees off its axis).
    fn pixel(&self, p: Vec3) -> Option<Vec2> {
        let rel = (p - self.eye).normalize_or_zero();
        let along = rel.dot(self.dir);
        if along < 0.05 {
            return None;
        }
        let ndc = Vec2::new(rel.dot(self.right), rel.dot(self.up)) / along / self.half_tan();
        Some(Vec2::new((ndc.x * 0.5 + 0.5) * SCREEN.x, (0.5 - ndc.y * 0.5) * SCREEN.y))
    }

    /// The point on the aiming sphere the camera sees at a HUD pixel.
    fn sphere_point(&self, px: Vec2) -> Vec3 {
        let ndc = Vec2::new(px.x / SCREEN.x * 2.0 - 1.0, 1.0 - px.y / SCREEN.y * 2.0) * self.half_tan();
        self.eye + (self.dir + self.right * ndc.x + self.up * ndc.y).normalize() * HUD_DIST
    }

    /// Where a controller ray aims: the HUD pixel for the game (if any) and the sphere point.
    fn aim(&self, from: Vec3, dir: Vec3) -> (Option<Vec2>, Vec3) {
        let hit = self.sphere_hit(from, dir);
        (self.pixel(hit), hit)
    }

    /// Moves HUD vertices drawn around pixel `at` onto the aiming sphere, as a flat card
    /// facing the rider at the point the camera sees `at`, the same size as on the screen.
    /// The result is still in HUD coordinates (now with depth), so it draws with the HUD.
    fn onto_sphere(&self, verts: &mut [gfx::Vertex], at: Vec2) {
        let centre = self.sphere_point(at);
        let fwd = (centre - self.eye).normalize();
        let right = fwd.cross(self.up).normalize();
        let up = right.cross(fwd);
        let per_px = 2.0 * self.half_tan().y * HUD_DIST / SCREEN.y;
        let to_hud = self.hud_to_world().inverse();
        for v in verts {
            let d = (Vec2::new(v.pos[0], v.pos[1]) - at) * per_px;
            v.pos = to_hud.transform_point3(centre + right * d.x - up * d.y).to_array();
        }
    }
}

/// Off-centre perspective for one eye, in the same conventions as `Mat4::perspective_rh`.
fn eye_projection(fov: xr::Fovf) -> Mat4 {
    let (l, r) = (fov.angle_left.tan(), fov.angle_right.tan());
    let (d, u) = (fov.angle_down.tan(), fov.angle_up.tan());
    let z = FAR / (NEAR - FAR);
    Mat4::from_cols(
        Vec4::new(2.0 / (r - l), 0.0, 0.0, 0.0),
        Vec4::new(0.0, 2.0 / (u - d), 0.0, 0.0),
        Vec4::new((r + l) / (r - l), (u + d) / (u - d), z, -1.0),
        Vec4::new(0.0, 0.0, z * NEAR, 0.0),
    )
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// The OpenXR runtime picks the GPU and creates the Vulkan instance and device for us.
unsafe fn create_vulkan(xr_instance: &xr::Instance, system: xr::SystemId) -> Result<gfx::Base, String> {
    let reqs = xr_instance.graphics_requirements::<xr::Vulkan>(system).map_err(|e| format!("graphics requirements: {e}"))?;
    let want = xr::Version::new(1, 1, 0);
    if want < reqs.min_api_version_supported || want.major() > reqs.max_api_version_supported.major() {
        return Err(format!(
            "the OpenXR runtime needs Vulkan {}..{}, the game uses 1.1",
            reqs.min_api_version_supported, reqs.max_api_version_supported
        ));
    }
    let entry = ash::Entry::load().map_err(|e| format!("Could not load Vulkan: {e}"))?;
    let get_proc = std::mem::transmute(entry.static_fn().get_instance_proc_addr);
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"Haunted Hollow")
        .engine_name(c"Haunted Hollow")
        .api_version(vk::API_VERSION_1_1);
    let ici = vk::InstanceCreateInfo::default().application_info(&app_info);
    let raw = xr_instance
        .create_vulkan_instance(system, get_proc, &ici as *const _ as *const _)
        .map_err(|e| format!("create Vulkan instance: {e}"))?
        .map_err(|e| format!("create Vulkan instance: {}", vk::Result::from_raw(e)))?;
    let instance = ash::Instance::load(entry.static_fn(), vk::Instance::from_raw(raw as _));
    let pdev = xr_instance
        .vulkan_graphics_device(system, instance.handle().as_raw() as _)
        .map_err(|e| format!("Vulkan device for the headset: {e}"))?;
    let pdev = vk::PhysicalDevice::from_raw(pdev as _);
    let qfi = instance
        .get_physical_device_queue_family_properties(pdev)
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or("the headset's GPU has no graphics queue")? as u32;
    let prio = [1.0f32];
    let qci = [vk::DeviceQueueCreateInfo::default().queue_family_index(qfi).queue_priorities(&prio)];
    let dci = vk::DeviceCreateInfo::default().queue_create_infos(&qci);
    let raw = xr_instance
        .create_vulkan_device(system, get_proc, pdev.as_raw() as _, &dci as *const _ as *const _)
        .map_err(|e| format!("create Vulkan device: {e}"))?
        .map_err(|e| format!("create Vulkan device: {}", vk::Result::from_raw(e)))?;
    let device = ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(raw as _));
    Ok(gfx::Base { entry, instance, pdev, qfi, device })
}

/// One eye's swapchain and the renderer targets wrapping its images.
struct EyeChain {
    swapchain: xr::Swapchain<xr::Vulkan>,
    targets: Vec<usize>,
    extent: vk::Extent2D,
}

struct Actions {
    set: xr::ActionSet,
    hands: [xr::Path; 2],
    aim: xr::Action<xr::Posef>,
    fire: xr::Action<bool>,
    start: xr::Action<bool>,
    pause: xr::Action<bool>,
    quit: xr::Action<bool>,
    rumble: xr::Action<xr::Haptic>,
}

impl Actions {
    fn new(inst: &xr::Instance) -> xr::Result<Actions> {
        let set = inst.create_action_set("gameplay", "Gameplay", 0)?;
        let hands = [inst.string_to_path("/user/hand/left")?, inst.string_to_path("/user/hand/right")?];
        let a = Actions {
            aim: set.create_action("aim", "Aim", &hands)?,
            fire: set.create_action("fire", "Shoot / join", &hands)?,
            start: set.create_action("start", "Start", &hands)?,
            pause: set.create_action("pause", "Pause", &hands)?,
            quit: set.create_action("back_to_title", "Back to the title screen (while paused)", &hands)?,
            rumble: set.create_action("rumble", "Rumble", &hands)?,
            set,
            hands,
        };
        // Suggested bindings: (profile, [(action, input path under /user/hand/<side>/)]).
        // The Steam Frame's controllers show up as Touch controllers.
        enum Act {
            Aim,
            Fire,
            Start,
            Pause,
            Quit,
            Rumble,
        }
        use Act::*;
        let profiles: [(&str, &[(Act, &str)]); 4] = [
            (
                "/interaction_profiles/oculus/touch_controller",
                &[
                    (Aim, "*/input/aim/pose"),
                    (Fire, "*/input/trigger/value"),
                    (Start, "right/input/a/click"),
                    (Start, "left/input/x/click"),
                    (Pause, "left/input/menu/click"),
                    (Quit, "right/input/b/click"),
                    (Quit, "left/input/y/click"),
                    (Rumble, "*/output/haptic"),
                ],
            ),
            (
                "/interaction_profiles/valve/index_controller",
                &[
                    (Aim, "*/input/aim/pose"),
                    (Fire, "*/input/trigger/click"),
                    (Start, "*/input/a/click"),
                    (Pause, "*/input/b/click"),
                    (Quit, "*/input/thumbstick/click"),
                    (Rumble, "*/output/haptic"),
                ],
            ),
            (
                "/interaction_profiles/htc/vive_controller",
                &[
                    (Aim, "*/input/aim/pose"),
                    (Fire, "*/input/trigger/click"),
                    (Start, "*/input/trackpad/click"),
                    (Pause, "*/input/menu/click"),
                    (Rumble, "*/output/haptic"),
                ],
            ),
            (
                "/interaction_profiles/khr/simple_controller",
                &[
                    (Aim, "*/input/aim/pose"),
                    (Fire, "*/input/select/click"),
                    (Start, "*/input/menu/click"),
                    (Rumble, "*/output/haptic"),
                ],
            ),
        ];
        for (profile, list) in profiles {
            let mut paths = vec![];
            for (act, input) in list {
                let sides: &[&str] = match input.split_once('/').unwrap().0 {
                    "*" => &["left", "right"],
                    "left" => &["left"],
                    _ => &["right"],
                };
                for side in sides {
                    let rest = input.split_once('/').unwrap().1;
                    paths.push((act, inst.string_to_path(&format!("/user/hand/{side}/{rest}"))?));
                }
            }
            let bindings: Vec<xr::Binding> = paths
                .iter()
                .map(|(act, path)| match act {
                    Aim => xr::Binding::new(&a.aim, *path),
                    Fire => xr::Binding::new(&a.fire, *path),
                    Start => xr::Binding::new(&a.start, *path),
                    Pause => xr::Binding::new(&a.pause, *path),
                    Quit => xr::Binding::new(&a.quit, *path),
                    Rumble => xr::Binding::new(&a.rumble, *path),
                })
                .collect();
            if let Err(e) = inst.suggest_interaction_profile_bindings(inst.string_to_path(profile)?, &bindings) {
                eprintln!("VR: no bindings for {profile}: {e}");
            }
        }
        Ok(a)
    }

    /// Per hand: (held, just pressed).
    fn button(&self, session: &xr::Session<xr::Vulkan>, action: &xr::Action<bool>) -> [(bool, bool); 2] {
        self.hands.map(|h| match action.state(session, h) {
            Ok(s) if s.is_active => (s.current_state, s.current_state && s.changed_since_last_sync),
            _ => (false, false),
        })
    }
}

/// Runs the game in VR until the runtime ends the session or Ctrl+C is pressed.
pub fn run(mut game: Game, static_mesh: &[gfx::Vertex], audio: &audio::Audio, timescale: f32) -> Result<(), String> {
    unsafe {
        libc::signal(libc::SIGINT, on_sigint as *const () as libc::sighandler_t);
    }
    #[cfg(target_arch = "aarch64")]
    let entry = xr::Entry::linked(&()).map_err(|e| format!("could not start the OpenXR loader: {e}"))?;
    #[cfg(not(target_arch = "aarch64"))]
    let entry = unsafe { xr::Entry::load(&()) }
        .map_err(|e| format!("could not load the OpenXR loader (libopenxr_loader.so.1): {e}"))?;
    let available = entry.enumerate_extensions().map_err(|e| format!("OpenXR extensions: {e}"))?;
    if !available.khr_vulkan_enable2 {
        return Err("the OpenXR runtime does not support Vulkan (XR_KHR_vulkan_enable2)".into());
    }
    let mut exts = xr::ExtensionSet::default();
    exts.khr_vulkan_enable2 = true;
    let inst = entry
        .create_instance(
            &xr::ApplicationInfo {
                application_name: "Haunted Hollow",
                application_version: 1,
                engine_name: "Haunted Hollow",
                engine_version: 1,
                api_version: xr::Version::new(1, 0, 0),
            },
            &exts,
            &[],
            &(),
        )
        .map_err(|e| format!("could not start OpenXR (is a runtime such as SteamVR running?): {e}"))?;
    if let Ok(p) = inst.properties() {
        eprintln!("OpenXR runtime: {} {}", p.runtime_name, p.runtime_version);
    }
    let system = inst
        .system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)
        .map_err(|e| format!("no headset found: {e}"))?;
    let blend_mode = inst
        .enumerate_environment_blend_modes(system, VIEW_TYPE)
        .map_err(|e| e.to_string())?
        .first()
        .copied()
        .unwrap_or(xr::EnvironmentBlendMode::OPAQUE);

    let base = unsafe { create_vulkan(&inst, system)? };
    // Declared before the session so it is dropped after it: the runtime must let go of the
    // swapchains before the Vulkan device goes away (locals drop in reverse order).
    let mut renderer_slot = None;
    let (session, mut frame_wait, mut frame_stream) = unsafe {
        inst.create_session::<xr::Vulkan>(
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: base.instance.handle().as_raw() as _,
                physical_device: base.pdev.as_raw() as _,
                device: base.device.handle().as_raw() as _,
                queue_family_index: base.qfi,
                queue_index: 0,
            },
        )
    }
    .map_err(|e| format!("could not start the VR session: {e}"))?;

    // An sRGB swapchain encodes what the shaders write, so they output linear colour
    // (fog.w = 2); with only UNORM formats on offer the colours pass straight through.
    let formats = session.enumerate_swapchain_formats().map_err(|e| e.to_string())?;
    let pick = [vk::Format::R8G8B8A8_SRGB, vk::Format::B8G8R8A8_SRGB, vk::Format::R8G8B8A8_UNORM, vk::Format::B8G8R8A8_UNORM]
        .into_iter()
        .find(|f| formats.contains(&(f.as_raw() as _)))
        .ok_or("the OpenXR runtime offers no 8-bit RGBA swapchain format")?;
    let linear = matches!(pick, vk::Format::R8G8B8A8_SRGB | vk::Format::B8G8R8A8_SRGB);

    let atlas = atlas::build();
    let renderer: &mut gfx::Renderer = renderer_slot.insert(gfx::Renderer::new_xr(base, pick, &atlas, atlas::SIZE)?);
    eprintln!("Vulkan device: {}", renderer.device_name);
    renderer.set_static_mesh(static_mesh)?;

    let views = inst.enumerate_view_configuration_views(system, VIEW_TYPE).map_err(|e| e.to_string())?;
    let mut eyes = vec![];
    for v in &views {
        let extent = vk::Extent2D { width: v.recommended_image_rect_width, height: v.recommended_image_rect_height };
        let swapchain = session
            .create_swapchain(&xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT,
                format: pick.as_raw() as _,
                sample_count: 1,
                width: extent.width,
                height: extent.height,
                face_count: 1,
                array_size: 1,
                mip_count: 1,
            })
            .map_err(|e| format!("VR swapchain: {e}"))?;
        let mut targets = vec![];
        for img in swapchain.enumerate_images().map_err(|e| e.to_string())? {
            targets.push(renderer.add_xr_target(vk::Image::from_raw(img as _), extent)?);
        }
        eyes.push(EyeChain { swapchain, targets, extent });
    }
    eprintln!("VR: {} views of {}x{}", eyes.len(), eyes[0].extent.width, eyes[0].extent.height);

    let actions = Actions::new(&inst).map_err(|e| format!("VR controller actions: {e}"))?;
    session.attach_action_sets(&[&actions.set]).map_err(|e| e.to_string())?;
    let aim_spaces = actions
        .hands
        .map(|h| actions.aim.create_space(&session, h, xr::Posef::IDENTITY).map_err(|e| e.to_string()));
    let [left_space, right_space] = aim_spaces;
    let aim_spaces = [left_space?, right_space?];
    // Seated: tracking space starts at the rider's head, which sits where the camera is.
    let local = session
        .create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)
        .map_err(|e| e.to_string())?;

    eprintln!("VR: pull the trigger to join; A/X starts, B/Y goes back (while paused), menu pauses. Ctrl+C quits.");
    let mut events = xr::EventDataBuffer::new();
    let mut running = false;
    let mut focused = false;
    let mut exiting = false;
    let mut hand = 1; // the hand that aims: whichever last pulled its trigger
    let mut last = Instant::now();
    let mut sim_time = 0.0f32;
    let mut fps_acc = (0.0f32, 0u32);
    let mut dyn_buf: Vec<gfx::Vertex> = Vec::with_capacity(200_000);
    let mut mist_buf: Vec<gfx::Vertex> = Vec::with_capacity(4096);

    let mut frames = || -> Result<(), String> {
        'main: loop {
            if QUIT.load(Ordering::Relaxed) && !exiting {
                exiting = true;
                if session.request_exit().is_err() {
                    break;
                }
            }
            while let Some(event) = inst.poll_event(&mut events).map_err(|e| e.to_string())? {
                match event {
                    xr::Event::SessionStateChanged(e) => {
                        focused = e.state() == xr::SessionState::FOCUSED;
                        match e.state() {
                            xr::SessionState::READY => {
                                session.begin(VIEW_TYPE).map_err(|e| e.to_string())?;
                                running = true;
                            }
                            xr::SessionState::STOPPING => {
                                session.end().map_err(|e| e.to_string())?;
                                running = false;
                            }
                            xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => break 'main,
                            _ => {}
                        }
                    }
                    xr::Event::InstanceLossPending(_) => break 'main,
                    _ => {}
                }
            }
            if !running {
                std::thread::sleep(std::time::Duration::from_millis(50));
                last = Instant::now();
                continue;
            }

            let state = frame_wait.wait().map_err(|e| e.to_string())?;
            frame_stream.begin().map_err(|e| e.to_string())?;
            let time = state.predicted_display_time;
            let now = Instant::now();
            let real_dt = (now - last).as_secs_f32().min(0.1);
            last = now;
            fps_acc.0 += real_dt;
            fps_acc.1 += 1;
            if fps_acc.0 > 0.5 {
                game.fps = fps_acc.1 as f32 / fps_acc.0;
                fps_acc = (0.0, 0);
            }

            // Controllers, measured against the camera as it was last frame.
            session.sync_actions(&[(&actions.set).into()]).map_err(|e| e.to_string())?;
            let fire = actions.button(&session, &actions.fire);
            if let Some(h) = (0..2).find(|&h| fire[h].1) {
                hand = h;
            }
            let pressed = |a: &xr::Action<bool>| actions.button(&session, a).iter().any(|b| b.1);
            let f = game.frame();
            let cart = Cart::new(f.cam, f.dir, f.up);
            let located = |h: usize| {
                aim_spaces[h].locate(&local, time).ok().filter(|l| {
                    l.location_flags
                        .contains(xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID)
                })
            };
            if located(hand).is_none() && located(1 - hand).is_some() {
                hand = 1 - hand;
            }
            let pointer = located(hand).map(|l| {
                let (pos, rot) = cart.to_world(&l.pose);
                (pos, rot * Vec3::NEG_Z)
            });
            let input = SourceInput {
                id: SourceId::Vr,
                aim: pointer.and_then(|(o, d)| cart.aim(o, d).0),
                fire: fire[hand].0,
                fire_pressed: fire[hand].1,
                join: fire[hand].1,
                start: pressed(&actions.start),
                pause: pressed(&actions.pause),
                quit_to_title: pressed(&actions.quit),
            };
            // The world holds still while the runtime has the rider's attention (its menus).
            if focused {
                sim_time += step_game(&mut game, audio, &[input], real_dt * timescale, SCREEN, None);
            }
            for (src, ms) in game.rumble_requests.drain(..) {
                if src == SourceId::Vr {
                    let buzz = xr::HapticVibration::new()
                        .amplitude(0.6)
                        .frequency(xr::FREQUENCY_UNSPECIFIED)
                        .duration(xr::Duration::from_nanos(ms as i64 * 1_000_000));
                    let _ = actions.rumble.apply_feedback(&session, actions.hands[hand], &buzz);
                }
            }
            game.led_requests.clear();

            if !state.should_render {
                frame_stream.end(time, blend_mode, &[]).map_err(|e| e.to_string())?;
                continue;
            }

            // Scene and HUD, built once and drawn for both eyes.
            let f = game.frame();
            let cart = Cart::new(f.cam, f.dir, f.up);
            dyn_buf.clear();
            game.draw_world(&mut dyn_buf);
            if let Some(l) = located(hand) {
                let (pos, rot) = cart.to_world(&l.pose);
                draw_pointer(&mut dyn_buf, &cart, pos, rot);
            }
            mist_buf.clear();
            game.draw_mist(&mut mist_buf);
            let mut ui = hud::Ui::new(SCREEN.x, SCREEN.y);
            game.draw_hud(&mut ui, false);
            for i in 0..game.players.len() {
                let start = ui.v.len();
                if let Some(at) = game.draw_crosshair(i, &mut ui) {
                    cart.onto_sphere(&mut ui.v[start..], at);
                }
            }

            let (_, views) = session.locate_views(VIEW_TYPE, time, &local).map_err(|e| e.to_string())?;
            let gamma = if linear { 2.0 } else { 1.0 };
            let clear = if linear { f.fog.map(srgb_to_linear) } else { f.fog };
            let hud = cart.hud_to_world();
            let mut draw = vec![];
            for (eye, view) in eyes.iter_mut().zip(&views) {
                let idx = eye.swapchain.acquire_image().map_err(|e| e.to_string())?;
                eye.swapchain.wait_image(xr::Duration::INFINITE).map_err(|e| e.to_string())?;
                let (pos, rot) = cart.to_world(&view.pose);
                let viewproj = eye_projection(view.fov) * Mat4::from_rotation_translation(rot, pos).inverse();
                let world = gfx::PushConstants {
                    viewproj: viewproj.to_cols_array_2d(),
                    cam: [pos.x, pos.y, pos.z, sim_time],
                    fog: [f.fog[0], f.fog[1], f.fog[2], gamma],
                    params: [f.fog_range.0, f.fog_range.1, f.lamp, 20.0],
                };
                let ui = gfx::PushConstants { viewproj: (viewproj * hud).to_cols_array_2d(), ..world };
                draw.push(gfx::Eye { target: eye.targets[idx as usize], world, ui });
            }
            renderer.draw_xr(&draw, &dyn_buf, &mist_buf, &ui.v, clear);
            for eye in eyes.iter_mut() {
                eye.swapchain.release_image().map_err(|e| e.to_string())?;
            }

            let rect = |e: &EyeChain| xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di { width: e.extent.width as _, height: e.extent.height as _ },
            };
            let layer_views: Vec<_> = eyes
                .iter()
                .zip(&views)
                .map(|(e, v)| {
                    xr::CompositionLayerProjectionView::new()
                        .pose(v.pose)
                        .fov(v.fov)
                        .sub_image(xr::SwapchainSubImage::new().swapchain(&e.swapchain).image_array_index(0).image_rect(rect(e)))
                })
                .collect();
            let layer = xr::CompositionLayerProjection::new().space(&local).views(&layer_views);
            frame_stream.end(time, blend_mode, &[&layer]).map_err(|e| e.to_string())?;
        }

        Ok(())
    };
    let result = frames();
    renderer.wait_idle();
    result
}

/// The controller as a little wand, with a beam in the rider's colour to show where it points.
fn draw_pointer(out: &mut Vec<gfx::Vertex>, cart: &Cart, pos: Vec3, rot: Quat) {
    let fwd = rot * Vec3::NEG_Z;
    let wand = Mat4::from_scale_rotation_translation(Vec3::new(0.035, 0.035, 0.14), rot, pos + fwd * 0.03);
    mesh::push_box(out, &wand, atlas::T_DARKPLANK, [1.0, 1.0, 1.0], Vec3::splat(0.6), 0.6);
    let end = cart.sphere_hit(pos, fwd);
    let len = pos.distance(end);
    let beam = Mat4::from_scale_rotation_translation(
        Vec3::new(0.01, 0.01, len),
        Quat::from_rotation_arc(Vec3::NEG_Z, fwd),
        (pos + end) * 0.5,
    );
    mesh::push_box(out, &beam, atlas::T_WHITE, PLAYER_COLORS[0], Vec3::ONE, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cart() -> (Cart, Mat4) {
        let (eye, dir, up) = (Vec3::new(3.0, 1.65, -7.0), Vec3::new(0.3, -0.2, 1.0).normalize(), Vec3::new(0.1, 1.0, 0.05));
        let game_viewproj = Mat4::perspective_rh(FOV_Y.to_radians(), SCREEN.x / SCREEN.y, NEAR, FAR) * Mat4::look_to_rh(eye, dir, up);
        (Cart::new(eye, dir, up), game_viewproj)
    }

    fn to_pixel(viewproj: Mat4, p: Vec3) -> Vec2 {
        let c = viewproj * p.extend(1.0);
        Vec2::new((c.x / c.w * 0.5 + 0.5) * SCREEN.x, (0.5 - c.y / c.w * 0.5) * SCREEN.y)
    }

    #[test]
    fn hud_lines_up_with_the_game_camera() {
        let (cart, game_viewproj) = cart();
        for px in [Vec2::ZERO, Vec2::new(640.0, 360.0), Vec2::new(1100.0, 90.0), SCREEN] {
            let world = cart.hud_to_world().transform_point3(px.extend(0.0));
            assert!(to_pixel(game_viewproj, world).distance(px) < 0.05, "{px}");
            // Aiming straight from the camera through that point aims at that pixel, and the
            // crosshair sits on the sphere in the same direction.
            let (aim, hit) = cart.aim(cart.eye, (world - cart.eye).normalize());
            let aim = aim.unwrap();
            assert!(aim.distance(px) < 0.05, "{px} -> {aim}");
            assert!(hit.distance(cart.sphere_point(px)) < 1e-3 && (hit.distance(cart.eye) - HUD_DIST).abs() < 1e-3);
        }
    }

    #[test]
    fn crosshair_follows_the_sphere() {
        let (cart, game_viewproj) = cart();
        // From a controller off to the side, pointing well outside the screen: the beam still
        // ends on the sphere, and the game aims at the pixel the camera sees that point at.
        let hand = cart.eye + cart.right * 0.3 - cart.up * 0.4;
        let dir = (cart.dir + cart.right * 1.5).normalize();
        let (aim, hit) = cart.aim(hand, dir);
        assert!((hit.distance(cart.eye) - HUD_DIST).abs() < 1e-3);
        assert!((hit - hand).normalize().dot(dir) > 0.9999);
        let aim = aim.unwrap();
        assert!(aim.x > SCREEN.x && to_pixel(game_viewproj, hit).distance(aim) < 0.05, "{aim}");
        // Behind the camera there is no screen point to aim at.
        assert!(cart.aim(hand, -cart.dir).0.is_none());
        // The crosshair card lands centred on that point, facing the rider.
        let mut v = [gfx::Vertex { pos: [aim.x, aim.y, 0.0], uv: [0.0; 2], color: [1.0; 4] }; 2];
        v[1].pos[0] += 10.0;
        cart.onto_sphere(&mut v, aim);
        let world = |v: &gfx::Vertex| cart.hud_to_world().transform_point3(Vec3::from_array(v.pos));
        assert!(world(&v[0]).distance(hit) < 1e-3);
        assert!((world(&v[1]) - world(&v[0])).dot(hit - cart.eye).abs() < 1e-3);
    }

    #[test]
    fn head_pose_is_relative_to_the_cart() {
        let (cart, _) = cart();
        // Looking straight ahead from the start position is the game camera.
        let (pos, rot) = cart.to_world(&xr::Posef::IDENTITY);
        assert!(pos.distance(cart.eye) < 1e-5 && (rot * Vec3::NEG_Z).distance(cart.dir) < 1e-5);
        // A symmetric 90-degree eye at the camera sees what the game camera sees in the middle.
        let fov = xr::Fovf { angle_left: -0.785398, angle_right: 0.785398, angle_up: 0.785398, angle_down: -0.785398 };
        let vp = eye_projection(fov) * Mat4::from_rotation_translation(rot, pos).inverse();
        let ahead = cart.eye + cart.dir * 10.0;
        assert!(to_pixel(vp, ahead).distance(SCREEN * 0.5) < 0.05);
        let up = to_pixel(vp, ahead + cart.up);
        assert!(up.y < SCREEN.y * 0.5 && (up.x - SCREEN.x * 0.5).abs() < 0.05, "{up}");
    }
}
