struct Uniforms {
    viewport: vec4<f32>,
    animation: vec4<f32>,
    controls: vec4<f32>,
    clock: vec4<f32>,
    state: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -0.3333333, 0.6666667, -1.0);
    let p = mix(vec4<f32>(c.bg, k.wz), vec4<f32>(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-5)), d / (q.x + 1e-5), q.x);
}

fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let p = abs(fract(c.xxx + vec3<f32>(0.0, 0.6666667, 0.3333333)) * 6.0 - 3.0);
    return c.z * mix(vec3<f32>(1.0), clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

fn hue_rotate(c: vec3<f32>, turn: f32) -> vec3<f32> {
    var h = rgb2hsv(c);
    h.x = fract(h.x + turn);
    return hsv2rgb(h);
}

fn screen(base: vec3<f32>, light: vec3<f32>, alpha: f32) -> vec3<f32> {
    return 1.0 - (1.0 - base) * (1.0 - light * clamp(alpha, 0.0, 1.0));
}

fn alt_progress(time: f32, duration: f32, delay: f32) -> f32 {
    let cycle = fract((time - delay) / (duration * 2.0)) * 2.0;
    return select(cycle, 2.0 - cycle, cycle > 1.0);
}

fn ease(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn key4(p: f32, times: vec2<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    if p < times.x { return mix(a, b, ease(p / times.x)); }
    if p < times.y { return mix(b, c, ease((p - times.x) / (times.y - times.x))); }
    return mix(c, d, ease((p - times.y) / (1.0 - times.y)));
}

fn orb_alpha(distance_ratio: f32, strength: f32) -> f32 {
    let core = exp(-3.2 * distance_ratio * distance_ratio);
    let outer = 1.0 - smoothstep(0.82, 1.45, distance_ratio);
    return core * outer * strength;
}

fn radial(uv: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return 1.0 - smoothstep(0.0, radius, distance(uv, center));
}

fn sd_round_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

fn overlay_round_rect(color: vec3<f32>, point: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>, radius: f32) -> vec3<f32> {
    let d = sd_round_box(point - center, half_size, radius);
    let fill = (1.0 - smoothstep(-1.0, 1.0, d)) * 0.14;
    let border = (1.0 - smoothstep(0.0, 1.15, abs(d))) * 0.18;
    return mix(color, vec3<f32>(0.92, 0.90, 0.96), max(fill * 0.13, border));
}

fn draw_line(color: vec3<f32>, point: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width: f32, alpha: f32) -> vec3<f32> {
    let pa = point - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    let mask = 1.0 - smoothstep(width, width + 1.0, length(pa - ba * h));
    return mix(color, vec3<f32>(0.96, 0.94, 0.98), mask * alpha);
}

fn triangle_mask(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let d1 = (p.x-b.x)*(a.y-b.y)-(a.x-b.x)*(p.y-b.y);
    let d2 = (p.x-c.x)*(b.y-c.y)-(b.x-c.x)*(p.y-c.y);
    let d3 = (p.x-a.x)*(c.y-a.y)-(c.x-a.x)*(p.y-a.y);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    return select(1.0, 0.0, has_neg && has_pos);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let scale_factor = u.animation.z;
    let size = u.viewport.zw;
    let point = frag.xy / scale_factor;
    let vmax = max(size.x, size.y);
    let time = u.animation.x * u.animation.y;

    var color = vec3<f32>(0.035, 0.039, 0.075);
    let center_glow = radial(point, size * 0.5, vmax * 0.69);
    color = mix(color, vec3<f32>(0.082, 0.075, 0.165), center_glow * 0.72);
    color = screen(color, vec3<f32>(0.55, 0.33, 0.88), radial(point, size * vec2<f32>(0.20, 0.25), vmax * 0.30) * 0.19);
    color = screen(color, vec3<f32>(0.10, 0.66, 0.76), radial(point, size * vec2<f32>(0.78, 0.68), vmax * 0.34) * 0.15);

    let p1 = alt_progress(time, 23.0, 0.0);
    let k1 = key4(p1, vec2<f32>(0.36, 0.68), vec3<f32>(0.0,0.0,0.72), vec3<f32>(0.34,0.11,1.22), vec3<f32>(0.23,0.52,0.88), vec3<f32>(0.65,0.25,1.45));
    let c1 = vec2<f32>(0.08*vmax,0.05*vmax) + k1.xy*vmax;
    let a1 = orb_alpha(distance(point,c1)/(0.19*vmax*k1.z),0.30);
    color = screen(color,hue_rotate(vec3<f32>(0.79,0.47,1.0),p1*0.375),a1);

    let p2 = alt_progress(time, 27.0, -7.0);
    let k2 = key4(p2, vec2<f32>(0.43,0.73), vec3<f32>(0.0,0.0,1.30), vec3<f32>(-0.31,-0.39,0.74), vec3<f32>(-0.55,-0.13,1.40), vec3<f32>(-0.18,-0.61,0.85));
    let c2 = vec2<f32>(size.x+0.04*vmax,size.y+0.01*vmax)+k2.xy*vmax;
    let a2 = orb_alpha(distance(point,c2)/(0.22*vmax*k2.z),0.16);
    color = screen(color,hue_rotate(vec3<f32>(0.20,0.91,0.87),0.18+p2*0.45),a2);

    let p3 = alt_progress(time,25.0,-13.0);
    let k3 = key4(p3,vec2<f32>(0.32,0.66),vec3<f32>(-0.22,0.18,0.62),vec3<f32>(0.28,-0.31,1.40),vec3<f32>(0.49,0.27,0.75),vec3<f32>(-0.35,-0.12,1.32));
    let c3 = vec2<f32>(size.x*0.32+0.17*vmax,size.y*0.29+0.17*vmax)+k3.xy*vmax;
    let a3 = orb_alpha(distance(point,c3)/(0.17*vmax*k3.z),0.26);
    color = screen(color,hue_rotate(vec3<f32>(1.0,0.52,0.72),0.44+p3*0.49),a3);

    let p4 = alt_progress(time,30.0,-9.0);
    let k4 = key4(p4,vec2<f32>(0.35,0.72),vec3<f32>(-0.15,0.05,1.25),vec3<f32>(-0.52,0.44,0.68),vec3<f32>(-0.15,0.69,1.45),vec3<f32>(-0.66,0.18,0.70));
    let c4 = vec2<f32>(size.x*0.87-0.155*vmax,0.045*vmax)+k4.xy*vmax;
    let a4 = orb_alpha(distance(point,c4)/(0.155*vmax*k4.z),0.23);
    color = screen(color,hue_rotate(vec3<f32>(1.0,0.79,0.36),0.69+p4*0.49),a4);

    let menu_progress = u.state.w;
    let menu_open = menu_progress > 0.5;
    let panel_w = clamp(size.x * 0.28, 260.0, 320.0);
    let panel_offset = (menu_progress - 1.0) * panel_w;
    let menu_point = point - vec2<f32>(panel_offset, 0.0);
    if menu_progress > 0.001 {
        let panel_mask = 1.0 - smoothstep(panel_w - 1.5, panel_w + 1.5, menu_point.x);
        color = mix(color, vec3<f32>(0.025, 0.028, 0.060), panel_mask * 0.76);
        color = draw_line(color, menu_point, vec2<f32>(panel_w, 0.0), vec2<f32>(panel_w, size.y), 1.0, 0.28);

        let row_half = vec2<f32>(panel_w * 0.5 - 24.0, 24.0);
        color = overlay_round_rect(color, menu_point, vec2<f32>(panel_w * 0.5, 190.0), row_half, 16.0);
        color = overlay_round_rect(color, menu_point, vec2<f32>(panel_w * 0.5, 282.0), row_half, 16.0);
        color = overlay_round_rect(color, menu_point, vec2<f32>(panel_w * 0.5, 374.0), row_half, 16.0);
        color = overlay_round_rect(color, menu_point, vec2<f32>(panel_w * 0.5, 466.0), row_half, 16.0);
        color = overlay_round_rect(color, menu_point, vec2<f32>(panel_w * 0.5, 558.0), row_half, 16.0);

        let track_a = vec2<f32>(42.0, 282.0);
        let track_b = vec2<f32>(panel_w - 42.0, 282.0);
        color = draw_line(color, menu_point, track_a, track_b, 1.25, 0.44);
        let speed_thumb = mix(track_a.x, track_b.x, u.controls.x);
        let sm = 1.0 - smoothstep(6.0, 7.0, distance(menu_point, vec2<f32>(speed_thumb, 282.0)));
        color = mix(color, vec3<f32>(0.86, 0.76, 1.0), sm);

        let size_track_a = vec2<f32>(42.0, 374.0);
        let size_track_b = vec2<f32>(panel_w - 42.0, 374.0);
        color = draw_line(color, menu_point, size_track_a, size_track_b, 1.25, 0.44);
        let size_thumb = mix(size_track_a.x, size_track_b.x, u.controls.y);
        let zm = 1.0 - smoothstep(6.0, 7.0, distance(menu_point, vec2<f32>(size_thumb, 374.0)));
        color = mix(color, vec3<f32>(0.86, 0.76, 1.0), zm);

        let toggle_center = vec2<f32>(panel_w - 57.0, 466.0);
        color = overlay_round_rect(color, menu_point, toggle_center, vec2<f32>(15.0, 8.0), 8.0);
        let toggle_x = toggle_center.x + mix(-7.0, 7.0, u.animation.w);
        let tm = 1.0 - smoothstep(5.0, 6.0, distance(menu_point, vec2<f32>(toggle_x, 466.0)));
        color = mix(color, vec3<f32>(0.92, 0.89, 0.98), tm);

        let thought_toggle_center = vec2<f32>(panel_w - 57.0, 558.0);
        color = overlay_round_rect(color, menu_point, thought_toggle_center, vec2<f32>(15.0, 8.0), 8.0);
        let thought_toggle_x = thought_toggle_center.x + mix(-7.0, 7.0, u.state.z);
        let thought_toggle_mask = 1.0 - smoothstep(5.0, 6.0, distance(menu_point, vec2<f32>(thought_toggle_x, 558.0)));
        color = mix(color, vec3<f32>(0.92, 0.89, 0.98), thought_toggle_mask);
    }

    let line_half=105.0*u.state.y;
    let line_x=abs(point.x-size.x*0.5);
    let line_alpha=(1.0-smoothstep(0.0,line_half,line_x))*(1.0-smoothstep(0.5,1.5,abs(point.y-u.state.x)))*0.46*u.state.z;
    color=mix(color,vec3<f32>(0.96,0.94,0.92),line_alpha);

    let menu_center = vec2<f32>(40.0, 40.0);
    color = overlay_round_rect(color, point, menu_center, vec2<f32>(20.0), 20.0);
    if menu_open {
        color = draw_line(color, point, menu_center + vec2<f32>(-6.0, -6.0), menu_center + vec2<f32>(6.0, 6.0), 1.2, 0.82);
        color = draw_line(color, point, menu_center + vec2<f32>(6.0, -6.0), menu_center + vec2<f32>(-6.0, 6.0), 1.2, 0.82);
    } else {
        color = draw_line(color, point, menu_center + vec2<f32>(-7.0, -5.0), menu_center + vec2<f32>(7.0, -5.0), 1.2, 0.82);
        color = draw_line(color, point, menu_center + vec2<f32>(-7.0, 0.0), menu_center + vec2<f32>(7.0, 0.0), 1.2, 0.82);
        color = draw_line(color, point, menu_center + vec2<f32>(-7.0, 5.0), menu_center + vec2<f32>(7.0, 5.0), 1.2, 0.82);
    }

    let fs_center=vec2<f32>(size.x-40.0,40.0);
    color=overlay_round_rect(color,point,fs_center,vec2<f32>(20.0),20.0);
    let fi=9.0; let arm=5.0;
    color=draw_line(color,point,fs_center+vec2<f32>(-fi,-fi),fs_center+vec2<f32>(-fi+arm,-fi),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(-fi,-fi),fs_center+vec2<f32>(-fi,-fi+arm),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(fi,fi),fs_center+vec2<f32>(fi-arm,fi),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(fi,fi),fs_center+vec2<f32>(fi,fi-arm),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(fi,-fi),fs_center+vec2<f32>(fi-arm,-fi),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(fi,-fi),fs_center+vec2<f32>(fi,-fi+arm),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(-fi,fi),fs_center+vec2<f32>(-fi+arm,fi),1.1,0.72);
    color=draw_line(color,point,fs_center+vec2<f32>(-fi,fi),fs_center+vec2<f32>(-fi,fi-arm),1.1,0.72);

    if u.controls.z > 0.5 {
        let media_center=vec2<f32>(size.x-85.0,size.y-42.0);
        color=overlay_round_rect(color,point,media_center,vec2<f32>(65.0,23.0),23.0);
        let prev=media_center+vec2<f32>(-37.0,0.0);
        let play=media_center+vec2<f32>(0.0,0.0);
        let next=media_center+vec2<f32>(37.0,0.0);
        let play_bg=1.0-smoothstep(18.0,19.0,distance(point,play));
        color=mix(color,vec3<f32>(0.94,0.92,0.97),play_bg*0.10);
        color=draw_line(color,point,prev+vec2<f32>(-7.0,-7.0),prev+vec2<f32>(-7.0,7.0),1.2,0.78);
        let prev_tri=triangle_mask(point,prev+vec2<f32>(-5.0,0.0),prev+vec2<f32>(6.0,-7.0),prev+vec2<f32>(6.0,7.0));
        color=mix(color,vec3<f32>(0.96,0.94,0.98),prev_tri*0.78);
        color=draw_line(color,point,next+vec2<f32>(7.0,-7.0),next+vec2<f32>(7.0,7.0),1.2,0.78);
        let next_tri=triangle_mask(point,next+vec2<f32>(5.0,0.0),next+vec2<f32>(-6.0,-7.0),next+vec2<f32>(-6.0,7.0));
        color=mix(color,vec3<f32>(0.96,0.94,0.98),next_tri*0.78);
        if u.controls.z > 1.5 {
            let pause_a=1.0-smoothstep(0.0,1.0,sd_round_box(point-(play+vec2<f32>(-3.7,0.0)),vec2<f32>(1.4,7.0),1.0));
            let pause_b=1.0-smoothstep(0.0,1.0,sd_round_box(point-(play+vec2<f32>(3.7,0.0)),vec2<f32>(1.4,7.0),1.0));
            color=mix(color,vec3<f32>(0.98,0.96,1.0),max(pause_a,pause_b));
        } else {
            let play_tri=triangle_mask(point,play+vec2<f32>(-5.0,-8.0),play+vec2<f32>(8.0,0.0),play+vec2<f32>(-5.0,8.0));
            color=mix(color,vec3<f32>(0.98,0.96,1.0),play_tri);
        }
    }

    let grain=hash21(floor(frag.xy)+floor(u.animation.x*5.0)*vec2<f32>(31.0,17.0))-0.5;
    color=clamp(color+grain*0.015,vec3<f32>(0.0),vec3<f32>(1.0));
    return vec4<f32>(color,1.0);
}
