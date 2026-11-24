// Regenerates ui/src/theme.rs from prism_ui's seededColorScheme (ColorStyle.vivid):
//   dart ui/tools/gen_theme.dart > ui/src/theme.rs   (from the repository root)
// Mirrors plugins/prism_ui/lib/color.dart; levels per .agents/surfaces.md.
import 'dart:math';

import '../../plugins/prism_ui/lib/hct/dynamic_color.dart';
import '../../plugins/prism_ui/lib/hct/dynamic_scheme.dart';
import '../../plugins/prism_ui/lib/hct/hct.dart';
import '../../plugins/prism_ui/lib/hct/material_dynamic_colors.dart';
import '../../plugins/prism_ui/lib/hct/tonal_palette.dart';

const seeds = [0xFF007AFF, 0xFFCB30E0, 0xFFFF2D55, 0xFFFF383C, 0xFFFF8D28, 0xFFFFCC00, 0xFF34C759, 0xFF8E8E93];
const fields = ['primary', 'on_primary', 'primary_container', 'on_primary_container', 'page', 'card', 'block', 'inset', 'on_surface', 'on_surface_variant', 'outline_variant', 'error'];

String hx(int v) => '0x${(v & 0xFFFFFF).toRadixString(16).padLeft(6, '0')}';
double clamp(double v, double a, double b) => v < a ? a : (v > b ? b : v);

Map<String, int> palette(int seed, bool dark) {
  final source = Hct.fromInt(seed);
  final scheme = DynamicScheme.content(sourceColorHct: source, isDark: dark);
  int neutral(DynamicColor role, double maxChroma, [double? darkTone]) {
    final p = role.palette(scheme);
    return Hct.from(p.hue, min(p.chroma, maxChroma), (dark ? darkTone : null) ?? role.getTone(scheme)).toInt();
  }
  int role(DynamicColor c) => c.getArgb(scheme);
  final paper = dark ? 3.0 : 1.5;
  final tone = dark ? clamp(source.tone + 3, 50, 75) : clamp(source.tone, 35, 60);
  final primary = tone == source.tone ? seed : Hct.from(source.hue, source.chroma, tone).toInt();
  final onPrimary = tone <= 60 ? 0xFFFFFFFF : TonalPalette.of(source.hue, source.chroma).get(10);
  final pp = scheme.primaryPalette;
  final container = TonalPalette.of(pp.hue, min(pp.chroma, 48.0));
  final surface = neutral(MaterialDynamicColors.surface, paper);
  final bright = neutral(MaterialDynamicColors.surfaceBright, paper);
  final c = neutral(MaterialDynamicColors.surfaceContainer, 3, 15);
  final high = neutral(MaterialDynamicColors.surfaceContainerHigh, 3, 21);
  return {
    'primary': primary,
    'on_primary': onPrimary,
    'primary_container': container.get(dark ? 30 : 90),
    'on_primary_container': container.get(dark ? 90 : 30),
    'page': dark ? surface : c,
    'card': dark ? c : bright,
    'block': dark ? high : c,
    'inset': neutral(MaterialDynamicColors.surfaceContainerHighest, 3, 27),
    'on_surface': dark ? MaterialDynamicColors.onSurface.palette(scheme).get(97) : role(MaterialDynamicColors.onSurface),
    'on_surface_variant': role(MaterialDynamicColors.onSurfaceVariant),
    'outline_variant': neutral(MaterialDynamicColors.outlineVariant, 4),
    'error': role(MaterialDynamicColors.error),
  };
}

void main() {
  final out = StringBuffer()
    ..writeln("//! Generated from prism_ui's seededColorScheme (plugins/prism_ui/lib/color.dart, ColorStyle.vivid,")
    ..writeln("//! FlClash's default) for each of lib/common/constant.dart's defaultPrimaryColors, light then dark.")
    ..writeln('//! Levels follow .agents/surfaces.md: page/card/block/inset.')
    ..writeln()
    ..writeln('/// The accents users pick from, as in `defaultPrimaryColors`.')
    ..writeln('pub const ACCENTS: [u32; 8] = [0x007AFF, 0xCB30E0, 0xFF2D55, 0xFF383C, 0xFF8D28, 0xFFCC00, 0x34C759, 0x8E8E93];')
    ..writeln()
    ..writeln('#[derive(Clone, Copy, Debug, PartialEq)]')
    ..writeln('pub struct Palette {');
  for (final f in fields) {
    out.writeln('    pub $f: u32,');
  }
  out
    ..writeln('}')
    ..writeln()
    ..writeln('/// `[accent][0 light, 1 dark]`.')
    ..writeln('pub const SCHEMES: [[Palette; 2]; 8] = [');
  for (final seed in seeds) {
    out.writeln('    [');
    for (final dark in [false, true]) {
      final p = palette(seed, dark);
      out.writeln('        Palette { ${fields.map((f) => '$f: ${hx(p[f]!)}').join(', ')} },');
    }
    out.writeln('    ],');
  }
  out.write('];');
  print(out);
}
