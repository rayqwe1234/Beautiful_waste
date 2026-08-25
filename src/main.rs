#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env, fs,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use bytemuck::{Pod, Zeroable};
use chrono::{Datelike, Local, Timelike};
use glyphon::{
    Attrs, Buffer, Cache, Color as TextColor, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
    cosmic_text::{Weight, Wrap},
};
use wgpu::{
    CommandEncoderDescriptor, CompositeAlphaMode, DeviceDescriptor, Instance, InstanceDescriptor,
    LoadOp, MultisampleState, Operations, PresentMode, RenderPassColorAttachment,
    RenderPassDescriptor, RequestAdapterOptions, SurfaceColorSpace, SurfaceConfiguration,
    TextureFormat, TextureUsages, TextureViewDescriptor,
    util::{BufferInitDescriptor, DeviceExt},
};
#[cfg(target_os = "windows")]
use winit::platform::windows::{WindowAttributesExtWindows, WindowExtWindows};
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{Fullscreen, Icon, Window},
};

const THOUGHTS: [&str; 24] = [
    "此刻，不必成為任何人。",
    "世界正在安靜地經過。",
    "留白也是一種風景。",
    "沒有目的，也可以很迷人。",
    "讓時間替你呼吸。",
    "今天的風，沒有要去哪裡。",
    "光落下來，時間沒有聲音。",
    "慢一點，也仍然會抵達。",
    "窗外的雲正在替你發呆。",
    "不必解釋每一段安靜。",
    "把今天留一小塊給自己。",
    "夜色知道如何擁抱城市。",
    "有些答案，晚一點來也很好。",
    "風景不需要被立刻命名。",
    "現在這樣，就已經足夠。",
    "讓心跳跟著光慢慢走。",
    "每一次呼吸，都是一段留白。",
    "世界很大，這一刻很小。",
    "柔軟不是退讓，是選擇。",
    "沒有安排的時間，也很珍貴。",
    "把匆忙放在門外一會兒。",
    "你可以只是看著光變化。",
    "時間不是催促，它只是流動。",
    "此刻正在成為一種風景。",
];
const WEEKDAYS: [&str; 7] = [
    "星期日",
    "星期一",
    "星期二",
    "星期三",
    "星期四",
    "星期五",
    "星期六",
];
const STYLE_NAMES: [&str; 5] = ["SANS", "SERIF", "MONO", "ROUND", "THIN"];
const DATE_FORMAT_NAMES: [&str; 3] = ["CHINESE", "ISO", "SLASH"];
const THOUGHT_DURATION: f32 = 11.0;

fn ease_opacity(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn thought_visual(elapsed: f32, duration: f32, count: usize, random: bool) -> (usize, u8) {
    let duration = duration.max(3.0);
    let phase = elapsed % duration;
    let cycle = (elapsed / duration).floor() as u64;
    let count = count.max(1);
    let index = if random {
        let mut value = cycle.wrapping_add(0x9E37_79B9_7F4A_7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((value ^ (value >> 31)) as usize) % count
    } else {
        cycle as usize % count
    };
    let fade_in = ease_opacity(phase / 0.85);
    let fade_out = 1.0 - ease_opacity((phase - (duration - 1.0)) / 1.0);
    (index, (fade_in * fade_out * 230.0) as u8)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    viewport: [f32; 4],
    animation: [f32; 4],
    controls: [f32; 4],
    clock: [f32; 4],
    state: [f32; 4],
}

#[derive(Clone, Copy, Default)]
struct HitRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
impl HitRect {
    fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.x + self.w
            && point[1] >= self.y
            && point[1] <= self.y + self.h
    }
}

#[derive(Clone, Copy, Default)]
struct UiLayout {
    menu: HitRect,
    menu_panel: HitRect,
    style: HitRect,
    speed: HitRect,
    size: HitRect,
    time_format: HitRect,
    seconds: HitRect,
    date_format: HitRect,
    thoughts: HitRect,
    thought_interval: HitRect,
    thought_order: HitRect,
    thought_input: HitRect,
    thought_prev: HitRect,
    thought_delete: HitRect,
    thought_next: HitRect,
    thought_add: HitRect,
    thought_import: HitRect,
    delete_user_files: HitRect,
    fullscreen: HitRect,
    previous: HitRect,
    playback: HitRect,
    next: HitRect,
}
impl UiLayout {
    fn new(width: f32, height: f32, media_visible: bool, menu_open: bool) -> Self {
        let panel_width = (width * 0.28).clamp(260.0, 320.0);
        let mut layout = Self {
            menu: HitRect {
                x: 20.0,
                y: 20.0,
                w: 40.0,
                h: 40.0,
            },
            menu_panel: HitRect {
                x: 0.0,
                y: 0.0,
                w: panel_width,
                h: height,
            },
            fullscreen: HitRect {
                x: width - 60.0,
                y: 20.0,
                w: 40.0,
                h: 40.0,
            },
            ..Default::default()
        };
        if menu_open {
            layout.style = HitRect {
                x: 36.0,
                y: 196.0,
                w: panel_width - 72.0,
                h: 40.0,
            };
            layout.size = HitRect {
                x: 36.0,
                y: 270.0,
                w: panel_width - 72.0,
                h: 56.0,
            };
            layout.time_format = HitRect {
                x: 36.0,
                y: 362.0,
                w: panel_width - 72.0,
                h: 42.0,
            };
            layout.seconds = HitRect {
                x: 36.0,
                y: 442.0,
                w: panel_width - 72.0,
                h: 42.0,
            };
            layout.date_format = HitRect {
                x: 36.0,
                y: 520.0,
                w: panel_width - 72.0,
                h: 42.0,
            };
            layout.speed = HitRect {
                x: 24.0,
                y: 614.0,
                w: panel_width - 48.0,
                h: 56.0,
            };
            layout.thoughts = HitRect {
                x: 36.0,
                y: 747.0,
                w: panel_width - 72.0,
                h: 42.0,
            };
            layout.thought_interval = HitRect {
                x: 36.0,
                y: 822.0,
                w: panel_width - 72.0,
                h: 56.0,
            };
            layout.thought_order = HitRect {
                x: 36.0,
                y: 910.0,
                w: panel_width - 72.0,
                h: 42.0,
            };
            let library_button_width = (panel_width - 88.0) / 3.0;
            layout.thought_prev = HitRect {
                x: 36.0,
                y: 1059.0,
                w: library_button_width,
                h: 42.0,
            };
            layout.thought_delete = HitRect {
                x: 44.0 + library_button_width,
                y: 1059.0,
                w: library_button_width,
                h: 42.0,
            };
            layout.thought_next = HitRect {
                x: 52.0 + library_button_width * 2.0,
                y: 1059.0,
                w: library_button_width,
                h: 42.0,
            };
            layout.thought_input = HitRect {
                x: 36.0,
                y: 1143.0,
                w: panel_width - 72.0,
                h: 56.0,
            };
            let button_width = (panel_width - 84.0) * 0.5;
            layout.thought_add = HitRect {
                x: 36.0,
                y: 1205.0,
                w: button_width,
                h: 44.0,
            };
            layout.thought_import = HitRect {
                x: 48.0 + button_width,
                y: 1205.0,
                w: button_width,
                h: 44.0,
            };
            layout.delete_user_files = HitRect {
                x: 24.0,
                y: 1321.0,
                w: panel_width - 48.0,
                h: 52.0,
            };
        }
        if media_visible {
            let mx = width - 145.0;
            let my = height - 62.0;
            layout.previous = HitRect {
                x: mx,
                y: my + 5.0,
                w: 33.0,
                h: 33.0,
            };
            layout.playback = HitRect {
                x: mx + 35.0,
                y: my + 2.5,
                w: 38.0,
                h: 38.0,
            };
            layout.next = HitRect {
                x: mx + 75.0,
                y: my + 5.0,
                w: 33.0,
                h: 33.0,
            };
        }
        layout
    }
}

#[derive(Clone, Copy)]
enum DragTarget {
    Speed,
    Size,
    ThoughtInterval,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeleteStatus {
    Idle,
    Armed,
    Deleted,
    Failed,
}

struct AppState {
    started: Instant,
    speed: f32,
    clock_scale: f32,
    use_24h: bool,
    show_seconds: bool,
    date_format: usize,
    show_thoughts: bool,
    thought_duration: f32,
    thought_random: bool,
    thoughts: Vec<String>,
    thought_selected: usize,
    thought_draft: String,
    editing_thought: bool,
    control_down: bool,
    menu_scroll: f32,
    style: usize,
    menu_open: bool,
    menu_progress: f32,
    last_frame: Instant,
    fullscreen: bool,
    media_state: u8,
    cursor: [f32; 2],
    drag: Option<DragTarget>,
    delete_status: DeleteStatus,
    delete_status_until: Option<Instant>,
    dirty_text: bool,
    last_second: u32,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            speed: 1.0,
            clock_scale: 1.0,
            use_24h: true,
            show_seconds: false,
            date_format: 0,
            show_thoughts: true,
            thought_duration: THOUGHT_DURATION,
            thought_random: false,
            thoughts: THOUGHTS.iter().map(|item| (*item).to_owned()).collect(),
            thought_selected: 0,
            thought_draft: String::new(),
            editing_thought: false,
            control_down: false,
            menu_scroll: 0.0,
            style: 0,
            menu_open: false,
            menu_progress: 0.0,
            last_frame: Instant::now(),
            fullscreen: false,
            media_state: 0,
            cursor: [0.0; 2],
            drag: None,
            delete_status: DeleteStatus::Idle,
            delete_status_until: None,
            dirty_text: true,
            last_second: u32::MAX,
        }
    }
}

impl AppState {
    fn load_from_disk() -> Self {
        let mut state = Self::default();
        if let Some(path) = settings_path()
            && let Ok(contents) = fs::read_to_string(path)
        {
            for line in contents.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                match key.trim() {
                    "speed" => {
                        if let Ok(speed) = value.trim().parse::<f32>() {
                            state.speed = speed.clamp(0.25, 2.50);
                        }
                    }
                    "clock_scale" => {
                        if let Ok(scale) = value.trim().parse::<f32>() {
                            state.clock_scale = scale.clamp(0.70, 1.35);
                        }
                    }
                    "use_24h" => state.use_24h = value.trim() != "false",
                    "show_seconds" => state.show_seconds = value.trim() == "true",
                    "date_format" => {
                        if let Ok(format) = value.trim().parse::<usize>() {
                            state.date_format = format.min(DATE_FORMAT_NAMES.len() - 1);
                        }
                    }
                    "show_thoughts" => state.show_thoughts = value.trim() != "false",
                    "thought_duration" => {
                        if let Ok(duration) = value.trim().parse::<f32>() {
                            state.thought_duration = duration.clamp(6.0, 30.0);
                        }
                    }
                    "thought_random" => state.thought_random = value.trim() == "true",
                    "style" => {
                        if let Ok(style) = value.trim().parse::<usize>() {
                            state.style = style.min(STYLE_NAMES.len() - 1);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(path) = thoughts_path()
            && let Ok(contents) = fs::read_to_string(path)
        {
            let versioned = contents.lines().next() == Some("# Beautiful Waste phrases v2");
            let loaded: Vec<String> = contents
                .lines()
                .skip(if versioned { 1 } else { 0 })
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .take(200)
                .map(ToOwned::to_owned)
                .collect();
            if versioned {
                if !loaded.is_empty() {
                    state.thoughts = loaded;
                }
            } else {
                for phrase in loaded {
                    if !state.thoughts.iter().any(|item| item == &phrase) {
                        state.thoughts.push(phrase);
                    }
                }
            }
        }
        state
    }
}

fn settings_path() -> Option<PathBuf> {
    let base = env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())?;
    Some(base.join("Beautiful Waste").join("settings.ini"))
}

fn thoughts_path() -> Option<PathBuf> {
    settings_path().and_then(|path| path.parent().map(|parent| parent.join("thoughts.txt")))
}

fn save_settings(state: &AppState) {
    let Some(path) = settings_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let contents = format!(
        "speed={:.4}\nclock_scale={:.4}\nuse_24h={}\nshow_seconds={}\ndate_format={}\nshow_thoughts={}\nthought_duration={:.2}\nthought_random={}\nstyle={}\n",
        state.speed,
        state.clock_scale,
        state.use_24h,
        state.show_seconds,
        state.date_format,
        state.show_thoughts,
        state.thought_duration,
        state.thought_random,
        state.style
    );
    let _ = fs::write(path, contents);
}

fn save_thoughts(state: &AppState) {
    let Some(path) = thoughts_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut contents = "# Beautiful Waste phrases v2\n".to_owned();
    contents.push_str(&state.thoughts.join("\n"));
    contents.push('\n');
    let _ = fs::write(path, contents);
}

fn remove_user_file(path: &PathBuf) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

fn delete_user_files(state: &mut AppState) -> bool {
    let settings = settings_path();
    let thoughts = thoughts_path();
    let settings_deleted = settings.as_ref().is_none_or(remove_user_file);
    let thoughts_deleted = thoughts.as_ref().is_none_or(remove_user_file);
    let deleted = settings_deleted && thoughts_deleted;

    if deleted {
        let defaults = AppState::default();
        state.speed = defaults.speed;
        state.clock_scale = defaults.clock_scale;
        state.use_24h = defaults.use_24h;
        state.show_seconds = defaults.show_seconds;
        state.date_format = defaults.date_format;
        state.show_thoughts = defaults.show_thoughts;
        state.thought_duration = defaults.thought_duration;
        state.thought_random = defaults.thought_random;
        state.thoughts = defaults.thoughts;
        state.thought_selected = 0;
        state.thought_draft.clear();
        state.editing_thought = false;
        state.style = defaults.style;
        state.drag = None;

        if let Some(parent) = settings.and_then(|path| path.parent().map(PathBuf::from)) {
            let _ = fs::remove_dir(parent);
        }
    }

    state.dirty_text = true;
    deleted
}

#[cfg(target_os = "windows")]
fn clipboard_text() -> Option<String> {
    use windows::Win32::{
        Foundation::HGLOBAL,
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
            Memory::{GlobalLock, GlobalUnlock},
        },
    };

    unsafe {
        OpenClipboard(None).ok()?;
        let text = (|| {
            let handle = GetClipboardData(13).ok()?;
            let memory = HGLOBAL(handle.0);
            let pointer = GlobalLock(memory) as *const u16;
            if pointer.is_null() {
                return None;
            }
            let mut length = 0usize;
            while length < 4096 && *pointer.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(memory);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}

#[cfg(not(target_os = "windows"))]
fn clipboard_text() -> Option<String> {
    None
}

fn append_thought_draft(state: &mut AppState, text: &str) {
    for character in text.chars() {
        if !character.is_control() && state.thought_draft.chars().count() < 20 {
            state.thought_draft.push(character);
        }
    }
    state.dirty_text = true;
}

fn commit_thought(state: &mut AppState) {
    let phrase = state.thought_draft.trim().to_owned();
    if !phrase.is_empty() && !state.thoughts.iter().any(|item| item == &phrase) {
        state.thoughts.push(phrase);
        state.thought_selected = state.thoughts.len() - 1;
        save_thoughts(state);
    }
    state.thought_draft.clear();
    state.dirty_text = true;
}

fn import_thoughts_from_txt(state: &mut AppState) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Text files", &["txt"])
        .set_title("Import phrases")
        .pick_file()
    else {
        return;
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let first_new = state.thoughts.len();
    for line in contents.lines() {
        let phrase: String = line.trim().chars().take(36).collect();
        if !phrase.is_empty() && !state.thoughts.iter().any(|item| item == &phrase) {
            state.thoughts.push(phrase);
        }
        if state.thoughts.len() >= 200 {
            break;
        }
    }
    if state.thoughts.len() > first_new {
        state.thought_selected = first_new;
        save_thoughts(state);
        state.dirty_text = true;
    }
}

fn phrase_preview(phrase: &str) -> String {
    let mut preview: String = phrase.chars().take(18).collect();
    if phrase.chars().count() > 18 {
        preview.push('…');
    }
    preview
}

struct TextItem {
    buffer: Buffer,
    left: f32,
    top: f32,
    color: TextColor,
}

struct Renderer {
    instance: Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_items: Vec<TextItem>,
    menu_text_start: usize,
    menu_text_end: usize,
    thought_text_item: Option<usize>,
    thought_index: usize,
    uniform: Uniforms,
    logical_size: [f32; 2],
    scale_factor: f32,
    occluded: bool,
    has_valid_size: bool,
    window: Arc<Window>,
}

impl Renderer {
    async fn new(window: Arc<Window>, event_loop: &ActiveEventLoop) -> Self {
        let physical = window.inner_size();
        let scale_factor = window.scale_factor() as f32;
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        )));
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .unwrap();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        // Shader colors intentionally use the original CSS sRGB values directly.
        let format = TextureFormat::Bgra8Unorm;
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical.width.max(1),
            height: physical.height.max(1),
            present_mode: PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let uniform = Uniforms::zeroed();
        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("visual uniforms"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("visual uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("visual uniform group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("beautiful waste visual shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("visual.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("visual pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("visual pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, MultisampleState::default(), None);
        Self {
            instance,
            device,
            queue,
            surface,
            config,
            pipeline,
            uniform_buffer,
            uniform_group,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_items: Vec::new(),
            menu_text_start: 0,
            menu_text_end: 0,
            thought_text_item: None,
            thought_index: usize::MAX,
            uniform,
            logical_size: [
                physical.width as f32 / scale_factor,
                physical.height as f32 / scale_factor,
            ],
            scale_factor,
            occluded: false,
            has_valid_size: physical.width > 0 && physical.height > 0,
            window,
        }
    }

    fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        if width == 0 || height == 0 {
            self.has_valid_size = false;
            return;
        }
        self.has_valid_size = true;
        self.config.width = width;
        self.config.height = height;
        self.scale_factor = scale_factor;
        self.logical_size = [width as f32 / scale_factor, height as f32 / scale_factor];
        // A borderless-fullscreen transition can briefly emit Occluded(true)
        // even though the window is still visible. Only suppress surface work
        // for a real minimization, otherwise the newly exposed area stays black
        // until Windows eventually emits Occluded(false).
        if !self.window.is_minimized().unwrap_or(false) {
            self.occluded = false;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn build_text(&mut self, state: &AppState) {
        self.text_items.clear();
        self.thought_text_item = None;
        let [width, height] = self.logical_size;
        let now = Local::now();
        let central_scale = state.clock_scale;
        let mut clock_size = (width * 0.1575).clamp(75.0, 184.0) * central_scale;
        let (family, weight, spacing) = match state.style {
            1 => (Family::Name("Times New Roman"), Weight::NORMAL, -0.055),
            2 => {
                clock_size = (width * 0.13).clamp(64.0, 152.0) * central_scale;
                (Family::Name("Cascadia Mono"), Weight::LIGHT, -0.10)
            }
            3 => (Family::Name("Arial Rounded MT Bold"), Weight::BOLD, -0.08),
            4 => (Family::Name("Segoe UI"), Weight::THIN, 0.01),
            _ => (Family::Name("Segoe UI"), Weight::LIGHT, -0.06),
        };
        let clock_top = height * 0.5 - 47.0 - clock_size * 0.53;
        let display_hour = if state.use_24h {
            now.hour()
        } else {
            let hour = now.hour() % 12;
            if hour == 0 { 12 } else { hour }
        };
        let parts: Vec<String> = if state.show_seconds {
            vec![
                format!("{display_hour:02}"),
                format!("{:02}", now.minute()),
                format!("{:02}", now.second()),
            ]
        } else {
            vec![format!("{display_hour:02}"), format!("{:02}", now.minute())]
        };
        let mut built = Vec::new();
        for part in &parts {
            built.push(make_text(
                &mut self.font_system,
                part,
                clock_size,
                clock_size * 0.92,
                family,
                weight,
                spacing,
            ));
        }
        let sep_width = clock_size * 0.23;
        let total =
            built.iter().map(|(_, w)| *w).sum::<f32>() + sep_width * (built.len() - 1) as f32;
        let mut x = width * 0.5 - total * 0.5;
        for (index, (buffer, item_width)) in built.into_iter().enumerate() {
            self.text_items.push(TextItem {
                buffer,
                left: x,
                top: clock_top,
                color: TextColor::rgba(245, 240, 235, 246),
            });
            x += item_width;
            if index + 1 < parts.len() {
                x += sep_width;
            }
        }
        let eyebrow_size = (10.5 * central_scale).clamp(8.0, 13.0);
        self.push_centered(
            "A BEAUTIFUL WASTE OF TIME",
            width * 0.5,
            clock_top - 28.0 * central_scale,
            eyebrow_size,
            "Segoe UI",
            Weight::MEDIUM,
            0.38,
            TextColor::rgba(245, 240, 235, 132),
        );
        let date_y = clock_top + clock_size + 8.0 * central_scale;
        let weekday = WEEKDAYS[now.weekday().num_days_from_sunday() as usize];
        let date_text = match state.date_format {
            1 => format!(
                "{:04}-{:02}-{:02}　{}",
                now.year(),
                now.month(),
                now.day(),
                weekday
            ),
            2 => format!(
                "{:04} / {:02} / {:02}　{}",
                now.year(),
                now.month(),
                now.day(),
                weekday
            ),
            _ => format!(
                "{} 年 {:02} 月 {:02} 日　{}",
                now.year(),
                now.month(),
                now.day(),
                weekday
            ),
        };
        self.push_centered(
            &date_text,
            width * 0.5,
            date_y,
            (16.0 * central_scale).clamp(12.0, 20.0),
            "HarmonyOS Sans SC",
            Weight::NORMAL,
            0.19,
            TextColor::rgba(245, 240, 235, 190),
        );
        let line_y = date_y + 62.0 * central_scale;
        let (thought_index, alpha) = thought_visual(
            state.started.elapsed().as_secs_f32(),
            state.thought_duration,
            state.thoughts.len(),
            state.thought_random,
        );
        if state.show_thoughts {
            let thought = state.thoughts[thought_index].as_str();
            let thought_text_item = self.text_items.len();
            self.push_centered(
                thought,
                width * 0.5,
                line_y + 40.0 * central_scale,
                (19.0 * central_scale).clamp(14.0, 24.0),
                "HarmonyOS Sans SC",
                Weight::NORMAL,
                0.17,
                TextColor::rgba(245, 240, 235, alpha),
            );
            self.thought_text_item = Some(thought_text_item);
        }
        self.thought_index = thought_index;

        let layout = UiLayout::new(width, height, state.media_state > 0, true);
        let menu_offset = (state.menu_progress - 1.0) * (width * 0.28).clamp(260.0, 320.0);
        let menu_y = -state.menu_scroll;
        self.menu_text_start = self.text_items.len();
        if state.menu_open || state.menu_progress > 0.001 {
            let menu_color = TextColor::rgba(245, 240, 235, 212);
            self.push_text(
                "SETTINGS",
                25.0 + menu_offset,
                72.0,
                20.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.menu_text_start = self.text_items.len();
            self.push_text(
                "CLOCK",
                25.0 + menu_offset,
                132.0 + menu_y,
                14.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.24,
                menu_color,
            );
            self.push_text(
                "STYLE",
                43.0 + menu_offset,
                175.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.26,
                menu_color,
            );
            self.push_centered(
                STYLE_NAMES[state.style],
                layout.style.x + layout.style.w * 0.5 + menu_offset,
                210.0 + menu_y,
                11.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.34,
                menu_color,
            );
            self.push_text(
                "CLOCK SIZE",
                43.0 + menu_offset,
                253.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_right(
                &format!("{:.0}%", state.clock_scale * 100.0),
                layout.size.x + layout.size.w - 12.0 + menu_offset,
                253.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_text(
                "TIME FORMAT",
                43.0 + menu_offset,
                341.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_text(
                if state.use_24h { "24 HOUR" } else { "12 HOUR" },
                layout.time_format.x + 13.0 + menu_offset,
                374.0 + menu_y,
                12.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                menu_color,
            );
            self.push_text(
                "SHOW SECONDS",
                43.0 + menu_offset,
                421.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_text(
                if state.show_seconds { "ON" } else { "OFF" },
                layout.seconds.x + 13.0 + menu_offset,
                454.0 + menu_y,
                13.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                menu_color,
            );
            self.push_text(
                "DATE FORMAT",
                43.0 + menu_offset,
                501.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_centered(
                DATE_FORMAT_NAMES[state.date_format],
                layout.date_format.x + layout.date_format.w * 0.5 + menu_offset,
                535.0 + menu_y,
                11.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.22,
                menu_color,
            );
            self.push_text(
                "ANIMATION SPEED",
                25.0 + menu_offset,
                592.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_right(
                &format!("{:.2}×", state.speed),
                layout.speed.x + layout.speed.w - 24.0 + menu_offset,
                592.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_text(
                "THOUGHTS",
                25.0 + menu_offset,
                682.0 + menu_y,
                14.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.24,
                menu_color,
            );
            self.push_text(
                "SHOW THOUGHTS",
                43.0 + menu_offset,
                726.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_text(
                if state.show_thoughts { "ON" } else { "OFF" },
                layout.thoughts.x + 13.0 + menu_offset,
                759.0 + menu_y,
                13.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                menu_color,
            );
            self.push_text(
                "CHANGE EVERY",
                43.0 + menu_offset,
                805.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_right(
                &format!("{:.0} S", state.thought_duration),
                layout.thought_interval.x + layout.thought_interval.w - 12.0 + menu_offset,
                805.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_right(
                &format!("{} PHRASES", state.thoughts.len()),
                layout.thoughts.x + layout.thoughts.w + menu_offset,
                685.0 + menu_y,
                8.5,
                "Segoe UI",
                Weight::MEDIUM,
                0.16,
                TextColor::rgba(245, 240, 235, 124),
            );
            self.push_text(
                "PLAYBACK ORDER",
                43.0 + menu_offset,
                891.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_text(
                if state.thought_random {
                    "RANDOM"
                } else {
                    "SEQUENTIAL"
                },
                layout.thought_order.x + 13.0 + menu_offset,
                922.0 + menu_y,
                11.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_text(
                "PHRASE LIBRARY",
                43.0 + menu_offset,
                971.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            self.push_text(
                &phrase_preview(&state.thoughts[state.thought_selected]),
                layout.thought_input.x + 13.0 + menu_offset,
                1020.0 + menu_y,
                10.5,
                "HarmonyOS Sans SC",
                Weight::NORMAL,
                0.04,
                menu_color,
            );
            self.push_centered(
                "PREV",
                layout.thought_prev.x + layout.thought_prev.w * 0.5 + menu_offset,
                1073.0 + menu_y,
                9.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_centered(
                "DELETE",
                layout.thought_delete.x + layout.thought_delete.w * 0.5 + menu_offset,
                1073.0 + menu_y,
                8.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.10,
                menu_color,
            );
            self.push_centered(
                "NEXT",
                layout.thought_next.x + layout.thought_next.w * 0.5 + menu_offset,
                1073.0 + menu_y,
                9.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.14,
                menu_color,
            );
            self.push_text(
                "CUSTOM TEXT",
                43.0 + menu_offset,
                1117.0 + menu_y,
                10.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.30,
                menu_color,
            );
            let draft_label = if state.thought_draft.is_empty() {
                "TYPE A PHRASE...".to_owned()
            } else if state.editing_thought {
                format!("{} |", state.thought_draft)
            } else {
                state.thought_draft.clone()
            };
            self.push_text(
                &draft_label,
                layout.thought_input.x + 13.0 + menu_offset,
                1166.0 + menu_y,
                10.5,
                "HarmonyOS Sans SC",
                Weight::NORMAL,
                0.04,
                if state.thought_draft.is_empty() {
                    TextColor::rgba(245, 240, 235, 104)
                } else {
                    menu_color
                },
            );
            self.push_centered(
                "ADD PHRASE",
                layout.thought_add.x + layout.thought_add.w * 0.5 + menu_offset,
                1221.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                menu_color,
            );
            self.push_centered(
                "IMPORT TXT",
                layout.thought_import.x + layout.thought_import.w * 0.5 + menu_offset,
                1221.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                menu_color,
            );
            self.push_text(
                "DATA",
                25.0 + menu_offset,
                1294.0 + menu_y,
                14.0,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.24,
                menu_color,
            );
            let (delete_label, delete_color) = match state.delete_status {
                DeleteStatus::Idle => ("DELETE USER FILES", menu_color),
                DeleteStatus::Armed => (
                    "CLICK AGAIN TO CONFIRM",
                    TextColor::rgba(255, 194, 194, 232),
                ),
                DeleteStatus::Deleted => {
                    ("USER FILES DELETED", TextColor::rgba(194, 240, 212, 224))
                }
                DeleteStatus::Failed => ("DELETE FAILED", TextColor::rgba(255, 178, 178, 232)),
            };
            self.push_centered(
                delete_label,
                layout.delete_user_files.x + layout.delete_user_files.w * 0.5 + menu_offset,
                1340.0 + menu_y,
                9.5,
                "Segoe UI",
                Weight::SEMIBOLD,
                0.16,
                delete_color,
            );
        }
        self.menu_text_end = self.text_items.len();

        self.uniform.clock = [
            clock_top,
            if state.thought_random { 1.0 } else { 0.0 },
            if state.use_24h { 1.0 } else { 0.0 },
            state.menu_scroll,
        ];
        self.uniform.state = [
            if state.show_thoughts { line_y } else { -1000.0 },
            central_scale,
            if state.show_thoughts { 1.0 } else { 0.0 },
            state.menu_progress,
        ];
    }

    fn push_centered(
        &mut self,
        text: &str,
        center_x: f32,
        top: f32,
        size: f32,
        family: &str,
        weight: Weight,
        spacing: f32,
        color: TextColor,
    ) {
        let (buffer, width) = make_text(
            &mut self.font_system,
            text,
            size,
            size * 1.22,
            Family::Name(family),
            weight,
            spacing,
        );
        self.text_items.push(TextItem {
            buffer,
            left: center_x - width * 0.5,
            top,
            color,
        });
    }
    fn push_text(
        &mut self,
        text: &str,
        left: f32,
        top: f32,
        size: f32,
        family: &str,
        weight: Weight,
        spacing: f32,
        color: TextColor,
    ) {
        let (buffer, _) = make_text(
            &mut self.font_system,
            text,
            size,
            size * 1.22,
            Family::Name(family),
            weight,
            spacing,
        );
        self.text_items.push(TextItem {
            buffer,
            left,
            top,
            color,
        });
    }

    fn push_right(
        &mut self,
        text: &str,
        right: f32,
        top: f32,
        size: f32,
        family: &str,
        weight: Weight,
        spacing: f32,
        color: TextColor,
    ) {
        let (buffer, width) = make_text(
            &mut self.font_system,
            text,
            size,
            size * 1.22,
            Family::Name(family),
            weight,
            spacing,
        );
        self.text_items.push(TextItem {
            buffer,
            left: right - width,
            top,
            color,
        });
    }

    fn render(&mut self, state: &mut AppState) {
        if !self.has_valid_size || self.occluded || self.window.is_minimized().unwrap_or(false) {
            return;
        }
        let frame_time = Instant::now();
        let delta = frame_time
            .duration_since(state.last_frame)
            .as_secs_f32()
            .min(0.10);
        state.last_frame = frame_time;
        let menu_target = if state.menu_open { 1.0 } else { 0.0 };
        if (state.menu_progress - menu_target).abs() > 0.001 {
            let smoothing = 1.0 - (-14.0 * delta).exp();
            state.menu_progress += (menu_target - state.menu_progress) * smoothing;
            state.dirty_text = true;
        } else {
            state.menu_progress = menu_target;
        }
        if let Some(deadline) = state.delete_status_until
            && frame_time >= deadline
        {
            state.delete_status = DeleteStatus::Idle;
            state.delete_status_until = None;
            state.dirty_text = true;
        }

        let now = Local::now();
        let elapsed = state.started.elapsed().as_secs_f32();
        let (thought_index, thought_alpha) = thought_visual(
            elapsed,
            state.thought_duration,
            state.thoughts.len(),
            state.thought_random,
        );
        if now.second() != state.last_second {
            state.last_second = now.second();
            state.dirty_text = true;
        }
        if state.show_thoughts && thought_index != self.thought_index {
            state.dirty_text = true;
        }
        if state.dirty_text {
            self.build_text(state);
            state.dirty_text = false;
        }
        if state.show_thoughts
            && let Some(item) = self
                .thought_text_item
                .and_then(|index| self.text_items.get_mut(index))
        {
            item.color = TextColor::rgba(245, 240, 235, thought_alpha);
        }
        let speed_norm = (state.speed - 0.25) / 2.25;
        let size_norm = (state.clock_scale - 0.70) / 0.65;
        self.uniform.viewport = [
            self.config.width as f32,
            self.config.height as f32,
            self.logical_size[0],
            self.logical_size[1],
        ];
        self.uniform.animation = [
            elapsed,
            state.speed,
            self.scale_factor,
            if state.show_seconds { 1.0 } else { 0.0 },
        ];
        self.uniform.controls = [
            speed_norm,
            size_norm,
            state.media_state as f32,
            (state.thought_duration - 6.0) / 24.0,
        ];
        self.uniform.clock[3] = state.menu_scroll;
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );
        let scale = self.scale_factor;
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: self.config.width as i32,
            bottom: self.config.height as i32,
        };
        let menu_bounds = TextBounds {
            left: 0,
            top: (105.0 * scale) as i32,
            right: ((self.logical_size[0] * 0.28).clamp(260.0, 320.0) * scale) as i32,
            bottom: self.config.height as i32,
        };
        let menu_text_start = self.menu_text_start;
        let menu_text_end = self.menu_text_end;
        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                self.text_items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| TextArea {
                        buffer: &item.buffer,
                        left: item.left * scale,
                        top: item.top * scale,
                        scale,
                        bounds: if index >= menu_text_start && index < menu_text_end {
                            menu_bounds
                        } else {
                            bounds
                        },
                        default_color: item.color,
                        custom_glyphs: &[],
                    }),
                &mut self.swash_cache,
            )
            .expect("prepare text");
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                // During a fullscreen style change this result is transient.
                // Keep asking for a frame instead of waiting for a delayed
                // WindowEvent::Occluded(false).
                self.occluded = self.window.is_minimized().unwrap_or(false);
                if !self.occluded {
                    self.window.request_redraw();
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => panic!("surface validation error"),
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("frame pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.draw(0..3, 0..1);
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .expect("render text");
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.atlas.trim();
        self.window.request_redraw();
    }
}

fn make_text(
    font_system: &mut FontSystem,
    text: &str,
    size: f32,
    line_height: f32,
    family: Family<'_>,
    weight: Weight,
    spacing: f32,
) -> (Buffer, f32) {
    let mut buffer = Buffer::new(font_system, Metrics::new(size, line_height));
    buffer.set_wrap(Wrap::None);
    buffer.set_size(Some(2400.0), Some(line_height * 1.5));
    buffer.set_text(
        text,
        &Attrs::new()
            .family(family)
            .weight(weight)
            .letter_spacing(spacing),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);
    let width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0_f32, f32::max);
    (buffer, width)
}

#[derive(Debug)]
enum UserEvent {
    Media(u8),
}

struct Application {
    renderer: Option<Renderer>,
    state: AppState,
    proxy: EventLoopProxy<UserEvent>,
}

impl Application {
    fn interact(&mut self, pressed: bool) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let layout = UiLayout::new(
            renderer.logical_size[0],
            renderer.logical_size[1],
            self.state.media_state > 0,
            self.state.menu_open,
        );
        if !pressed {
            if self.state.drag.take().is_some() {
                save_settings(&self.state);
            }
            return;
        }
        let p = self.state.cursor;
        let mut settings_p = p;
        settings_p[1] += self.state.menu_scroll;
        let mut settings_changed = false;
        if self.state.editing_thought && !layout.thought_input.contains(settings_p) {
            self.state.editing_thought = false;
            renderer.window.set_ime_allowed(false);
            self.state.dirty_text = true;
        }
        if layout.menu.contains(p) {
            self.state.menu_open = !self.state.menu_open;
            if !self.state.menu_open {
                self.state.editing_thought = false;
                self.state.delete_status = DeleteStatus::Idle;
                self.state.delete_status_until = None;
                renderer.window.set_ime_allowed(false);
            }
            self.state.dirty_text = true;
        } else if layout.style.contains(settings_p) {
            self.state.style = (self.state.style + 1) % STYLE_NAMES.len();
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.time_format.contains(settings_p) {
            self.state.use_24h = !self.state.use_24h;
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.seconds.contains(settings_p) {
            self.state.show_seconds = !self.state.show_seconds;
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.date_format.contains(settings_p) {
            self.state.date_format = (self.state.date_format + 1) % DATE_FORMAT_NAMES.len();
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.thoughts.contains(settings_p) {
            self.state.show_thoughts = !self.state.show_thoughts;
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.thought_order.contains(settings_p) {
            self.state.thought_random = !self.state.thought_random;
            self.state.dirty_text = true;
            settings_changed = true;
        } else if layout.thought_prev.contains(settings_p) {
            self.state.thought_selected = if self.state.thought_selected == 0 {
                self.state.thoughts.len() - 1
            } else {
                self.state.thought_selected - 1
            };
            self.state.dirty_text = true;
        } else if layout.thought_next.contains(settings_p) {
            self.state.thought_selected =
                (self.state.thought_selected + 1) % self.state.thoughts.len();
            self.state.dirty_text = true;
        } else if layout.thought_delete.contains(settings_p) {
            if self.state.thoughts.len() > 1 {
                self.state.thoughts.remove(self.state.thought_selected);
                self.state.thought_selected = self
                    .state
                    .thought_selected
                    .min(self.state.thoughts.len() - 1);
                save_thoughts(&self.state);
                self.state.dirty_text = true;
            }
        } else if layout.thought_input.contains(settings_p) {
            self.state.editing_thought = true;
            renderer.window.set_ime_allowed(true);
            self.state.dirty_text = true;
        } else if layout.thought_add.contains(settings_p) {
            commit_thought(&mut self.state);
        } else if layout.thought_import.contains(settings_p) {
            import_thoughts_from_txt(&mut self.state);
        } else if layout.delete_user_files.contains(settings_p) {
            let now = Instant::now();
            let confirmed = self.state.delete_status == DeleteStatus::Armed
                && self
                    .state
                    .delete_status_until
                    .is_some_and(|deadline| now < deadline);
            if confirmed {
                self.state.delete_status = if delete_user_files(&mut self.state) {
                    DeleteStatus::Deleted
                } else {
                    DeleteStatus::Failed
                };
                self.state.delete_status_until = Some(now + Duration::from_secs(3));
            } else {
                self.state.delete_status = DeleteStatus::Armed;
                self.state.delete_status_until = Some(now + Duration::from_secs(5));
                self.state.dirty_text = true;
            }
        } else if layout.fullscreen.contains(p) {
            self.state.fullscreen = !self.state.fullscreen;
            renderer.occluded = false;
            renderer.window.set_fullscreen(
                self.state
                    .fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            );
            renderer.window.request_redraw();
        } else if layout.speed.contains(settings_p) {
            self.state.drag = Some(DragTarget::Speed);
            update_slider(&mut self.state, layout, settings_p);
        } else if layout.size.contains(settings_p) {
            self.state.drag = Some(DragTarget::Size);
            update_slider(&mut self.state, layout, settings_p);
        } else if layout.thought_interval.contains(settings_p) {
            self.state.drag = Some(DragTarget::ThoughtInterval);
            update_slider(&mut self.state, layout, settings_p);
        } else if layout.previous.contains(p) {
            send_media_key(MediaKey::Previous)
        } else if layout.playback.contains(p) {
            send_media_key(MediaKey::Toggle)
        } else if layout.next.contains(p) {
            send_media_key(MediaKey::Next)
        } else if self.state.menu_open && !layout.menu_panel.contains(p) {
            self.state.menu_open = false;
            self.state.delete_status = DeleteStatus::Idle;
            self.state.delete_status_until = None;
            self.state.dirty_text = true;
        }
        if settings_changed {
            save_settings(&self.state);
        }
    }
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("美丽的废物 · Beautiful Waste")
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(520.0, 620.0));
        if let Some(icon) = load_icon() {
            attrs = attrs.with_window_icon(Some(icon.clone()));
            #[cfg(target_os = "windows")]
            {
                attrs = attrs.with_taskbar_icon(Some(icon));
            }
        }
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        #[cfg(target_os = "windows")]
        if let Some(icon) = load_icon() {
            window.set_window_icon(Some(icon.clone()));
            window.set_taskbar_icon(Some(icon));
        }
        #[cfg(target_os = "windows")]
        configure_windows_window_identity(&window);
        self.renderer = Some(pollster::block_on(Renderer::new(window, event_loop)));
        start_media_monitor(self.proxy.clone());
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Media(value) => {
                self.state.media_state = value;
                self.state.dirty_text = true;
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                renderer.resize(
                    size.width,
                    size.height,
                    renderer.window.scale_factor() as f32,
                );
                self.state.dirty_text = true;
                renderer.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = renderer.window.inner_size();
                renderer.resize(size.width, size.height, scale_factor as f32);
                self.state.dirty_text = true;
            }
            WindowEvent::Occluded(occluded) => {
                // Windows reports a short-lived occlusion while changing the
                // borderless-fullscreen window style. Treat it as suspended
                // rendering only when the window is actually minimized.
                renderer.occluded = occluded && renderer.window.is_minimized().unwrap_or(false);
                if !renderer.occluded {
                    let size = renderer.window.inner_size();
                    renderer.resize(
                        size.width,
                        size.height,
                        renderer.window.scale_factor() as f32,
                    );
                    self.state.dirty_text = true;
                    renderer.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                self.state.cursor = [
                    x as f32 / renderer.scale_factor,
                    y as f32 / renderer.scale_factor,
                ];
                if self.state.drag.is_some() {
                    let layout = UiLayout::new(
                        renderer.logical_size[0],
                        renderer.logical_size[1],
                        self.state.media_state > 0,
                        self.state.menu_open,
                    );
                    let cursor = self.state.cursor;
                    update_slider(&mut self.state, layout, cursor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.state.menu_open => {
                let panel_width = (renderer.logical_size[0] * 0.28).clamp(260.0, 320.0);
                if self.state.cursor[0] <= panel_width {
                    let amount = match delta {
                        MouseScrollDelta::LineDelta(_, y) => -y * 52.0,
                        MouseScrollDelta::PixelDelta(position) => {
                            -(position.y as f32 / renderer.scale_factor)
                        }
                    };
                    let max_scroll = (1397.0 - (renderer.logical_size[1] - 24.0)).max(0.0);
                    self.state.menu_scroll =
                        (self.state.menu_scroll + amount).clamp(0.0, max_scroll);
                    self.state.dirty_text = true;
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.interact(state == ElementState::Pressed),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.state.control_down = modifiers.state().control_key();
            }
            WindowEvent::Ime(Ime::Commit(value)) if self.state.editing_thought => {
                append_thought_draft(&mut self.state, &value);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.state.editing_thought {
                    match event.logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            self.state.thought_draft.pop();
                            self.state.dirty_text = true;
                        }
                        Key::Named(NamedKey::Enter) => {
                            commit_thought(&mut self.state);
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.state.editing_thought = false;
                            renderer.window.set_ime_allowed(false);
                            self.state.dirty_text = true;
                        }
                        Key::Character(ref value)
                            if self.state.control_down && value.eq_ignore_ascii_case("v") =>
                        {
                            if let Some(text) = clipboard_text() {
                                append_thought_draft(&mut self.state, &text);
                            }
                        }
                        Key::Character(ref value) => {
                            append_thought_draft(&mut self.state, value);
                        }
                        _ => {}
                    }
                } else {
                    match event.logical_key {
                        Key::Character(ref value) if value.eq_ignore_ascii_case("s") => {
                            self.state.style = (self.state.style + 1) % STYLE_NAMES.len();
                            self.state.dirty_text = true;
                            save_settings(&self.state);
                        }
                        Key::Named(NamedKey::Escape) if self.state.fullscreen => {
                            self.state.fullscreen = false;
                            renderer.occluded = false;
                            renderer.window.set_fullscreen(None);
                            renderer.window.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => renderer.render(&mut self.state),
            _ => {}
        }
    }
}

fn update_slider(state: &mut AppState, layout: UiLayout, point: [f32; 2]) {
    match state.drag {
        Some(DragTarget::Speed) => {
            let t =
                ((point[0] - (layout.speed.x + 18.0)) / (layout.speed.w - 36.0)).clamp(0.0, 1.0);
            state.speed = 0.25 + t * 2.25;
        }
        Some(DragTarget::Size) => {
            let t = ((point[0] - (layout.size.x + 18.0)) / (layout.size.w - 36.0)).clamp(0.0, 1.0);
            state.clock_scale = 0.70 + t * 0.65;
            state.dirty_text = true;
        }
        Some(DragTarget::ThoughtInterval) => {
            let t = ((point[0] - (layout.thought_interval.x + 18.0))
                / (layout.thought_interval.w - 36.0))
                .clamp(0.0, 1.0);
            state.thought_duration = 6.0 + t * 24.0;
            state.dirty_text = true;
        }
        None => {}
    }
}

fn load_icon() -> Option<Icon> {
    let image = image::load_from_memory(include_bytes!("../icon.ico"))
        .ok()?
        .into_rgba8();
    let (w, h) = image.dimensions();
    Icon::from_rgba(image.into_raw(), w, h).ok()
}

#[cfg(target_os = "windows")]
fn configure_windows_window_identity(window: &Window) {
    use windows::{
        Win32::{
            Foundation::{HWND, PROPERTYKEY},
            System::Com::StructuredStorage::PROPVARIANT,
            UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
        },
        core::GUID,
    };

    const APP_ID: &str = "ShenChengrui.BeautifulWaste";
    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };
    const PKEY_APP_USER_MODEL_RELAUNCH_ICON_RESOURCE: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 3,
    };

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    let Ok(executable) = env::current_exe() else {
        return;
    };
    let icon_resource = format!("{},0", executable.display());

    unsafe {
        let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) else {
            return;
        };
        let app_id = PROPVARIANT::from(APP_ID);
        let icon_resource = PROPVARIANT::from(icon_resource.as_str());
        let _ = store.SetValue(&PKEY_APP_USER_MODEL_ID, &app_id);
        let _ = store.SetValue(&PKEY_APP_USER_MODEL_RELAUNCH_ICON_RESOURCE, &icon_resource);
        let _ = store.Commit();
    }
}

fn start_media_monitor(proxy: EventLoopProxy<UserEvent>) {
    thread::spawn(move || {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
        }
        let mut old = u8::MAX;
        loop {
            let value = query_media_state();
            if value != old {
                let _ = proxy.send_event(UserEvent::Media(value));
                old = value;
            }
            thread::sleep(Duration::from_millis(750));
        }
    });
}

#[cfg(target_os = "windows")]
fn query_media_state() -> u8 {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let Ok(op) = Manager::RequestAsync() else {
        return 0;
    };
    let Ok(manager) = op.get() else { return 0 };
    let Ok(session) = manager.GetCurrentSession() else {
        return 0;
    };
    let Ok(info) = session.GetPlaybackInfo() else {
        return 0;
    };
    let Ok(status) = info.PlaybackStatus() else {
        return 0;
    };
    if status == Status::Playing {
        2
    } else if status == Status::Paused || status == Status::Stopped {
        1
    } else {
        0
    }
}
#[cfg(not(target_os = "windows"))]
fn query_media_state() -> u8 {
    0
}

enum MediaKey {
    Previous,
    Toggle,
    Next,
}
#[cfg(target_os = "windows")]
fn send_media_key(key: MediaKey) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK, keybd_event,
    };
    let code = match key {
        MediaKey::Previous => VK_MEDIA_PREV_TRACK,
        MediaKey::Toggle => VK_MEDIA_PLAY_PAUSE,
        MediaKey::Next => VK_MEDIA_NEXT_TRACK,
    };
    unsafe {
        keybd_event(code.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(code.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}
#[cfg(not(target_os = "windows"))]
fn send_media_key(_: MediaKey) {}

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            windows::core::w!("ShenChengrui.BeautifulWaste"),
        );
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let mut app = Application {
        renderer: None,
        state: AppState::load_from_disk(),
        proxy,
    };
    event_loop.run_app(&mut app).unwrap();
}
