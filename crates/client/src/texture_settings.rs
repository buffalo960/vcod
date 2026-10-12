//! The Graphics page's texture settings as retail's renderer applies them:
//! `r_picmip`, `r_picmip2` and `r_texturebits` at image load (latched, so a
//! change waits for `vid_restart`) and `r_textureMode` on the samplers
//! (docs/research/cod11-front-end.md, section 14, "Texture settings").

use vcod_common::assets::{self, Image, ImageData};
use vcod_common::pk3::Pk3Fs;

/// What one renderer build loads and samples images with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSettings {
    /// `r_picmip`: mip levels dropped from world images, 0..=3.
    pub picmip: u32,
    /// `r_picmip2`: the same for model images (Character Textures).
    pub picmip2: u32,
    /// `r_texturebits`: 16 stores uncompressed images at 16 bits a texel.
    pub bits: u32,
    pub filter: TextureFilter,
}

impl Default for TextureSettings {
    /// vcod's cvar defaults: every level kept, `r_texturebits` 0,
    /// `GL_LINEAR_MIPMAP_NEAREST`.
    fn default() -> Self {
        TextureSettings {
            picmip: 0,
            picmip2: 0,
            bits: 0,
            filter: TextureFilter::by_name("GL_LINEAR_MIPMAP_NEAREST").unwrap(),
        }
    }
}

impl TextureSettings {
    /// The settings `r_picmip`, `r_picmip2`, `r_texturebits` and
    /// `r_textureMode` read as; a filter name retail would refuse keeps
    /// `prev`'s filter, as `GL_TextureMode` leaves the images alone.
    pub fn from_cvars(
        picmip: f32,
        picmip2: f32,
        bits: f32,
        texture_mode: &str,
        prev: TextureFilter,
    ) -> TextureSettings {
        TextureSettings {
            picmip: picmip_level(picmip),
            picmip2: picmip_level(picmip2),
            bits: bits as i32 as u32,
            filter: TextureFilter::by_name(texture_mode).unwrap_or(prev),
        }
    }

    /// Levels to drop from an image of a world shader (`nopicmip` keeps
    /// every level).
    pub fn world_picmip(&self, nopicmip: bool) -> u32 {
        if nopicmip { 0 } else { self.picmip }
    }

    /// Levels to drop from a model's image: a shader registered without a
    /// lightmap starts with the `picmip2` flag (0x4fc6f3).
    pub fn model_picmip(&self, nopicmip: bool) -> u32 {
        if nopicmip { 0 } else { self.picmip2 }
    }
}

/// `0x4ea380`: the cvar's integer clamped to 0..=3.
fn picmip_level(v: f32) -> u32 {
    (v as i32).clamp(0, 3) as u32
}

/// One row of `GL_TextureMode`'s table (CoDMP.exe 0x5712f8): the min and
/// mag filters of a mipmapped image. `mip` `None` is a min filter without
/// mipmaps, which samples level 0 only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureFilter {
    pub min: wgpu::FilterMode,
    pub mag: wgpu::FilterMode,
    pub mip: Option<wgpu::MipmapFilterMode>,
}

impl TextureFilter {
    /// The six names `GL_TextureMode` (0x4ea530) accepts, compared without
    /// case; anything else prints `bad filter name`.
    pub fn by_name(name: &str) -> Option<TextureFilter> {
        use wgpu::FilterMode::{Linear, Nearest};
        use wgpu::MipmapFilterMode as M;
        let (min, mag, mip) = match name.to_ascii_uppercase().as_str() {
            "GL_NEAREST" => (Nearest, Nearest, None),
            "GL_LINEAR" => (Linear, Linear, None),
            "GL_NEAREST_MIPMAP_NEAREST" => (Nearest, Nearest, Some(M::Nearest)),
            "GL_LINEAR_MIPMAP_NEAREST" => (Linear, Linear, Some(M::Nearest)),
            "GL_NEAREST_MIPMAP_LINEAR" => (Nearest, Nearest, Some(M::Linear)),
            "GL_LINEAR_MIPMAP_LINEAR" => (Linear, Linear, Some(M::Linear)),
            _ => return None,
        };
        Some(TextureFilter { min, mag, mip })
    }

    /// A sampler descriptor for mipmapped images under this filter. vcod's
    /// 16x anisotropy rides only the trilinear mode, the one wgpu allows it
    /// with; retail 1.1 forces `r_ext_texture_filter_anisotropic` to 0.
    pub fn sampler<'a>(
        &self,
        label: &'a str,
        address: wgpu::AddressMode,
    ) -> wgpu::SamplerDescriptor<'a> {
        let trilinear = self.min == wgpu::FilterMode::Linear
            && self.mag == wgpu::FilterMode::Linear
            && self.mip == Some(wgpu::MipmapFilterMode::Linear);
        wgpu::SamplerDescriptor {
            label: Some(label),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: self.mag,
            min_filter: self.min,
            mipmap_filter: self.mip.unwrap_or(wgpu::MipmapFilterMode::Nearest),
            lod_max_clamp: if self.mip.is_some() { 32.0 } else { 0.0 },
            anisotropy_clamp: if trilinear { 16 } else { 1 },
            ..Default::default()
        }
    }
}

/// How an image's smaller levels are made: retail's image flag 1
/// (mipmapped, cleared by `nomipmaps`) and its `#N` custom levels.
#[derive(Clone, Copy, Default)]
pub struct Mips<'a> {
    /// Flag 1: build the chain down to 1x1 (0x4ebf20's mip loop).
    pub chain: bool,
    /// The image's name and a loader for a level by name. A name with
    /// `#<digits>` takes its next level from `#<digits + 1>` when that
    /// image exists at half the size (0x4eb1d0); picmip halving reads it
    /// too.
    pub custom: Option<(&'a str, &'a dyn Fn(&str) -> Option<Image>)>,
}

impl<'a> Mips<'a> {
    /// A mipmapped image, levels box filtered or taken from `#N` files.
    pub fn chain(name: &'a str, load: &'a dyn Fn(&str) -> Option<Image>) -> Self {
        Mips {
            chain: true,
            custom: Some((name, load)),
        }
    }

    /// `nomipmaps`: level 0 only; picmip still reads `#N` files.
    pub fn single(name: &'a str, load: &'a dyn Fn(&str) -> Option<Image>) -> Self {
        Mips {
            chain: false,
            custom: Some((name, load)),
        }
    }
}

/// A `#N` level by pk3 path, probed over the image extensions as
/// `R_LoadImage` probes them (0x4edf20).
pub fn path_levels(fs: &Pk3Fs) -> impl Fn(&str) -> Option<Image> + '_ {
    |name| assets::resolve_bundle_image(fs, name).map(|p| assets::load_path_image(fs, &p))
}

/// A `#N` level of a model skin, through `load_skin_image` so foliage masks
/// invert like their level 0.
pub fn skin_levels(fs: &Pk3Fs) -> impl Fn(&str) -> Option<Image> + '_ {
    |name| {
        assets::resolve_bundle_image(fs, &format!("skins/{name}"))
            .map(|p| assets::load_skin_image(fs, &p["skins/".len()..]))
    }
}

/// An image as retail uploads it: `picmip` levels dropped, the texture
/// gamma baked in (`gamma::bake_image`), the mip chain built from that,
/// then `r_texturebits` on every level. Block-compressed images lose whole
/// mip levels while both sides stay above 4 (0x4ef590) and keep their own
/// chain; uncompressed ones are halved to `size >> picmip` (0x4ebf20) and
/// stored at 16 bits a texel with `r_texturebits 16` (0x4ebe30). Returns
/// an uncompressed image's levels below level 0 (empty without `chain`).
pub fn prepare(
    img: &mut Image,
    picmip: u32,
    gamma: Option<&[u8; 256]>,
    bits: u32,
    mips: Mips<'_>,
) -> Vec<Vec<u8>> {
    let mut down = Down {
        name: mips.custom.map(|(n, _)| n.to_string()),
        load: mips.custom.map(|(_, l)| l),
    };
    drop_levels(img, picmip, &mut down);
    crate::gamma::bake_image(img, gamma);
    let ImageData::Rgba8(px) = &mut img.data else {
        return Vec::new();
    };
    let mut levels = Vec::new();
    if mips.chain {
        let (mut w, mut h) = (img.width, img.height);
        while w > 1 || h > 1 {
            let prev = levels.last().unwrap_or(&*px);
            let (nw, nh, next) = down.step(w, h, prev);
            levels.push(next);
            (w, h) = (nw, nh);
        }
    }
    if bits == 16 {
        // the driver converts each level as it is uploaded
        let opaque = px.iter().skip(3).step_by(4).all(|&a| a == 255);
        quantize_16(px, opaque);
        for l in &mut levels {
            quantize_16(l, opaque);
        }
    }
    levels
}

/// Retail's level-down step (0x4eb1d0): the next `#N` image when one
/// exists at the right size, else a 2x2 box.
struct Down<'a> {
    name: Option<String>,
    load: Option<&'a dyn Fn(&str) -> Option<Image>>,
}

impl Down<'_> {
    fn step(&mut self, w: u32, h: u32, px: &[u8]) -> (u32, u32, Vec<u8>) {
        if let (Some(name), Some(load)) = (&self.name, self.load)
            && let Some(next) = next_custom_level(name)
            && let Some(img) = load(&next)
        {
            // retail's (w + 1) >> 1; the GPU's chain halves down, so an odd
            // side (no retail image has one) keeps the box
            let want = (w.div_ceil(2), h.div_ceil(2));
            let fits = want == ((w / 2).max(1), (h / 2).max(1));
            match img.data {
                ImageData::Rgba8(level) if fits && (img.width, img.height) == want => {
                    self.name = Some(next);
                    return (want.0, want.1, level);
                }
                // retail refuses the whole image here; vcod box filters
                _ => log::warn!(
                    "custom mipmap image '{next}' should be {} x {}, is {} x {}; filtering instead",
                    want.0,
                    want.1,
                    img.width,
                    img.height
                ),
            }
        }
        halve(w, h, px)
    }
}

/// `a#3.tga` -> `a#4.tga`: the digits after the first `#` plus one, the
/// rest kept (0x4eb1d0, `"%i%s"` at 0x54bf78). `None` without a `#`.
fn next_custom_level(name: &str) -> Option<String> {
    let at = name.find('#')? + 1;
    let digits = name[at..].bytes().take_while(u8::is_ascii_digit).count();
    let n: i64 = name[at..at + digits].parse().unwrap_or(0);
    Some(format!("{}{}{}", &name[..at], n + 1, &name[at + digits..]))
}

fn drop_levels(img: &mut Image, picmip: u32, down: &mut Down) {
    match &mut img.data {
        ImageData::Bc { mips, .. } => {
            let mut drop = 0;
            let (mut w, mut h) = (img.width, img.height);
            while drop < picmip && w >= 5 && h >= 5 && mips.len() > drop as usize + 1 {
                w >>= 1;
                h >>= 1;
                drop += 1;
            }
            mips.drain(..drop as usize);
            img.width = w;
            img.height = h;
        }
        ImageData::Rgba8(px) => {
            for _ in 0..picmip {
                if img.width == 1 && img.height == 1 {
                    break;
                }
                let (w, h, half) = down.step(img.width, img.height, px);
                *px = half;
                img.width = w;
                img.height = h;
            }
        }
    }
}

/// One level down as 0x4eb1d0 builds it with `r_simpleMipMaps 1` and
/// `r_weightMipMaps 0` (their defaults): each texel the truncated mean of
/// a 2x2 block. An image one texel wide or high averages neighbouring
/// pairs instead.
fn halve(w: u32, h: u32, px: &[u8]) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    if w == 1 || h == 1 {
        for (o, pair) in out.chunks_exact_mut(4).zip(px.chunks_exact(8)) {
            for c in 0..4 {
                o[c] = ((u32::from(pair[c]) + u32::from(pair[4 + c])) >> 1) as u8;
            }
        }
        return (nw, nh, out);
    }
    let row = (w * 4) as usize;
    for (y, o_row) in out.chunks_exact_mut((nw * 4) as usize).enumerate() {
        let r0 = &px[2 * y * row..][..row];
        let r1 = &px[(2 * y + 1) * row..][..row];
        let pairs = r0.as_chunks::<8>().0.iter().zip(r1.as_chunks::<8>().0);
        for (o, (a, b)) in o_row.as_chunks_mut::<4>().0.iter_mut().zip(pairs) {
            for c in 0..4 {
                let sum =
                    u16::from(a[c]) + u16::from(a[4 + c]) + u16::from(b[c]) + u16::from(b[4 + c]);
                o[c] = (sum >> 2) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// `r_texturebits 16`: an image with any alpha below 255 goes up as
/// `GL_RGBA4`, an opaque one as `GL_RGB5` (0x4ebe30), every level by level
/// 0's alpha. The driver's rounding is not in the binary; this rounds to
/// nearest and widens back.
fn quantize_16(px: &mut [u8], opaque: bool) {
    let q = |v: u8, bits: u32| -> u8 {
        let max = (1u32 << bits) - 1;
        let n = (u32::from(v) * max + 127) / 255;
        ((n * 255 + max / 2) / max) as u8
    };
    for t in px.as_chunks_mut::<4>().0 {
        if opaque {
            for c in &mut t[..3] {
                *c = q(*c, 5);
            }
        } else {
            for c in t.iter_mut() {
                *c = q(*c, 4);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcod_common::assets::TextureFormat;

    fn rgba(w: u32, h: u32, px: Vec<u8>) -> Image {
        Image {
            width: w,
            height: h,
            data: ImageData::Rgba8(px),
        }
    }

    #[test]
    fn picmip_cvars_clamp_to_three_levels() {
        let f = TextureFilter::by_name("GL_LINEAR").unwrap();
        let s = TextureSettings::from_cvars(-1.0, 7.0, 16.0, "gl_linear", f);
        assert_eq!((s.picmip, s.picmip2, s.bits), (0, 3, 16));
        assert_eq!(s.world_picmip(false), 0);
        assert_eq!(s.model_picmip(false), 3);
        assert_eq!(s.model_picmip(true), 0);
    }

    #[test]
    fn texture_modes_map_to_retail_filters() {
        use wgpu::FilterMode::{Linear, Nearest};
        use wgpu::MipmapFilterMode as M;
        let bilinear = TextureFilter::by_name("GL_LINEAR_MIPMAP_NEAREST").unwrap();
        assert_eq!(
            (bilinear.min, bilinear.mag, bilinear.mip),
            (Linear, Linear, Some(M::Nearest))
        );
        let tri = TextureFilter::by_name("gl_linear_mipmap_linear").unwrap();
        assert_eq!(tri.mip, Some(M::Linear));
        let near = TextureFilter::by_name("GL_NEAREST").unwrap();
        assert_eq!((near.min, near.mip), (Nearest, None));
        assert_eq!(
            near.sampler("t", wgpu::AddressMode::Repeat).lod_max_clamp,
            0.0
        );
        assert_eq!(
            tri.sampler("t", wgpu::AddressMode::Repeat).anisotropy_clamp,
            16
        );
        assert_eq!(
            bilinear
                .sampler("t", wgpu::AddressMode::Repeat)
                .anisotropy_clamp,
            1
        );
        // a refused name keeps the filter already set
        let s = TextureSettings::from_cvars(1.0, 2.0, 0.0, "GL_CUBIC", tri);
        assert_eq!(s.filter, tri);
    }

    #[test]
    fn picmip_halves_uncompressed_images() {
        let mut img = rgba(4, 2, (0..32).map(|i| (i * 8) as u8).collect());
        prepare(&mut img, 1, None, 0, Mips::default());
        assert_eq!((img.width, img.height), (2, 1));
        let ImageData::Rgba8(px) = &img.data else {
            unreachable!()
        };
        // red of texels 0, 1, 4 and 5 (0, 32, 128, 160) averaged
        assert_eq!(px[0], 80);
        prepare(&mut img, 3, None, 0, Mips::default());
        assert_eq!((img.width, img.height), (1, 1));
    }

    #[test]
    fn picmip_drops_compressed_levels_while_sides_exceed_four() {
        let mips = (0..4).map(|l| vec![l as u8; 8]).collect();
        let mut img = Image {
            width: 16,
            height: 8,
            data: ImageData::Bc {
                format: TextureFormat::Bc1RgbaUnorm,
                mips,
            },
        };
        prepare(&mut img, 3, None, 16, Mips::default());
        // 16x8 -> 8x4, then 4 is not above 4
        assert_eq!((img.width, img.height), (8, 4));
        let ImageData::Bc { mips, .. } = &img.data else {
            unreachable!()
        };
        assert_eq!(mips.len(), 3);
        assert_eq!(mips[0][0], 1);
    }

    #[test]
    fn sixteen_bits_quantize_by_alpha() {
        let mut opaque = rgba(1, 1, vec![100, 200, 7, 255]);
        prepare(&mut opaque, 0, None, 16, Mips::default());
        let ImageData::Rgba8(px) = &opaque.data else {
            unreachable!()
        };
        // 5 bits: 100 -> 12 -> 99, 200 -> 24 -> 197, 7 -> 1 -> 8
        assert_eq!(px.as_slice(), &[99, 197, 8, 255]);
        let mut alpha = rgba(1, 1, vec![100, 200, 7, 128]);
        prepare(&mut alpha, 0, None, 16, Mips::default());
        let ImageData::Rgba8(px) = &alpha.data else {
            unreachable!()
        };
        // 4 bits: 100 -> 6 -> 102, 200 -> 12 -> 204, 7 -> 0, 128 -> 8 -> 136
        assert_eq!(px.as_slice(), &[102, 204, 0, 136]);
        let mut kept = rgba(1, 1, vec![100, 200, 7, 128]);
        prepare(&mut kept, 0, None, 32, Mips::default());
        let ImageData::Rgba8(px) = &kept.data else {
            unreachable!()
        };
        assert_eq!(px.as_slice(), &[100, 200, 7, 128]);
    }

    fn px(img: &Image) -> &[u8] {
        let ImageData::Rgba8(px) = &img.data else {
            unreachable!()
        };
        px
    }

    fn no_files(_: &str) -> Option<Image> {
        None
    }

    #[test]
    fn mip_chain_box_filters_down_to_one_texel() {
        // 4x2, red = texel index * 10, alpha 255
        let data = (0..8u8).flat_map(|i| [i * 10, 0, 0, 255]).collect();
        let mut img = rgba(4, 2, data);
        let levels = prepare(&mut img, 0, None, 0, Mips::chain("a", &no_files));
        assert_eq!(levels.len(), 2);
        // 2x1: (0 + 10 + 40 + 50) / 4 = 25, (20 + 30 + 60 + 70) / 4 = 45
        assert_eq!(levels[0], vec![25, 0, 0, 255, 45, 0, 0, 255]);
        // 1x1 from a one-high level: the pair's mean, truncated
        assert_eq!(levels[1], vec![35, 0, 0, 255]);
        assert_eq!(px(&img).len(), 32);
    }

    #[test]
    fn box_filter_truncates() {
        let mut img = rgba(2, 2, vec![1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2]);
        let levels = prepare(&mut img, 0, None, 0, Mips::chain("a", &no_files));
        // 3/4, 4/4, 5/4, 6/4
        assert_eq!(levels, vec![vec![0, 1, 1, 1]]);
    }

    #[test]
    fn nomipmaps_uploads_level_zero_only() {
        let mut img = rgba(2, 2, vec![9; 16]);
        assert!(prepare(&mut img, 0, None, 0, Mips::single("a", &no_files)).is_empty());
        assert!(prepare(&mut img, 0, None, 0, Mips::default()).is_empty());
    }

    #[test]
    fn custom_levels_come_from_the_next_numbered_image() {
        assert_eq!(
            next_custom_level("t/a@b#0.tga").as_deref(),
            Some("t/a@b#1.tga")
        );
        assert_eq!(next_custom_level("a#9").as_deref(), Some("a#10"));
        assert_eq!(next_custom_level("plain.tga"), None);
        // a#1 exists at half size, a#2 is missing
        let load = |name: &str| match name {
            "a#1.tga" => Some(rgba(2, 2, vec![200; 16])),
            _ => None,
        };
        let mut img = rgba(4, 4, vec![0; 64]);
        let levels = prepare(&mut img, 0, None, 0, Mips::chain("a#0.tga", &load));
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], vec![200; 16]);
        // the 1x1 level is filtered from the custom one
        assert_eq!(levels[1], vec![200; 4]);
        // a level of the wrong size is box filtered instead
        let wrong = |_: &str| Some(rgba(4, 4, vec![7; 64]));
        let mut img = rgba(4, 4, vec![0; 64]);
        let levels = prepare(&mut img, 0, None, 0, Mips::chain("a#0.tga", &wrong));
        assert_eq!(levels[0], vec![0; 16]);
    }

    #[test]
    fn picmip_reads_custom_levels_and_the_chain_continues_after_them() {
        let load = |name: &str| match name {
            "a#1" => Some(rgba(2, 2, vec![100; 16])),
            "a#2" => Some(rgba(1, 1, vec![50; 4])),
            _ => None,
        };
        let mut img = rgba(4, 4, vec![0; 64]);
        let levels = prepare(&mut img, 1, None, 0, Mips::chain("a#0", &load));
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(px(&img), &[100; 16]);
        assert_eq!(levels, vec![vec![50; 4]]);
        // without a chain, picmip still takes the custom level
        let mut img = rgba(4, 4, vec![0; 64]);
        assert!(prepare(&mut img, 1, None, 0, Mips::single("a#0", &load)).is_empty());
        assert_eq!(px(&img), &[100; 16]);
    }

    #[test]
    fn sixteen_bits_quantize_every_level_by_level_zero_alpha() {
        let mut img = rgba(2, 1, vec![100, 100, 100, 255, 100, 100, 100, 255]);
        let levels = prepare(&mut img, 0, None, 16, Mips::chain("a", &no_files));
        // opaque: 5 bits, 100 -> 99 on both levels
        assert_eq!(px(&img)[0], 99);
        assert_eq!(levels[0], vec![99, 99, 99, 255]);
    }
}
