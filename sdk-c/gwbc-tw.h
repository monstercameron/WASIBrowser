/* gwbc-tw.h — typed TailwindCSS utilities for gwbc (pluggable).
 *
 *   #include "gwbc.h"
 *   #include "gwbc-tw.h"
 *
 * Typed functions instead of string soup: colors are (TwColor, shade),
 * spacing is the Tailwind scale (n * 0.25rem), enums for sizes/weights/
 * shadows/rounding. Everything returns a utility Node and composes with the
 * core class engine (dedup into the generated stylesheet, Hover() works).
 *
 *   div(class(U(twBg(TwSlate, 100), twP(6), twRounded(TwRoundedXl),
 *              twShadow(TwShadowMd), twTextColor(TwSlate, 900))), ...)
 *
 * BASE CSS: including this header and using any tw* token auto-enables a
 * Preflight (Tailwind's reset, condensed) scoped to #mount — margins zeroed,
 * border-box sizing, unstyled buttons/lists/links, inherited fonts on form
 * controls. Opt out with  #define GWBC_TW_NO_PREFLIGHT  before including.
 *
 * Parity notes: palette + spacing + typography + flex/grid + borders +
 * shadows + opacity + overflow + position + transitions covered (classic hex
 * palette — near-identical to the oklch values, guaranteed to parse).
 * NOT yet: responsive/dark variants (needs media-query classes), gradients,
 * ring/divide/space-between utilities, arbitrary values beyond the scale fns.
 */
#ifndef GWBC_TW_H
#define GWBC_TW_H

/* ------------------------------------------------------------- preflight */

/* Every selector is wrapped in :where() so the whole reset has ZERO
 * specificity — utility classes (.uN) must always beat the preflight, same
 * as real Tailwind. A bare `#mount *` would be (1,0,0) and override them. */
static const char *gc_tw_preflight =
    ":where(#mount *){box-sizing:border-box;margin:0;padding:0;"
    "border:0 solid #e5e7eb}"
    ":where(#mount h1,#mount h2,#mount h3,#mount h4,#mount h5,#mount h6)"
    "{font-size:inherit;font-weight:inherit}"
    ":where(#mount button,#mount input,#mount select,#mount textarea)"
    "{font-family:inherit;font-size:inherit;color:inherit;background:transparent}"
    ":where(#mount button){cursor:pointer}"
    ":where(#mount ul,#mount ol){list-style:none}"
    ":where(#mount a){color:inherit;text-decoration:inherit}"
    ":where(#mount img,#mount video){max-width:100%;display:block}";

/* All tw tokens route through here: first use plugs the preflight into the
 * generated stylesheet. */
static Node gc_tw(const char *prop, const char *value) {
    if (!gc_base_css) {
#ifndef GWBC_TW_NO_PREFLIGHT
        gc_base_css = gc_tw_preflight;
        gc_css_dirty = 1;
#else
        gc_base_css = "";
#endif
    }
    return gc_u(prop, value);
}

/* --------------------------------------------------------------- palette */

typedef enum {
    TwSlate, TwGray, TwZinc, TwNeutral, TwStone,
    TwRed, TwOrange, TwAmber, TwYellow, TwLime,
    TwGreen, TwEmerald, TwTeal, TwCyan, TwSky,
    TwBlue, TwIndigo, TwViolet, TwPurple, TwFuchsia,
    TwPink, TwRose,
    TwColorCount
} TwColor;

/* shades: 50 100 200 300 400 500 600 700 800 900 950 */
static const char *gc_tw_palette[TwColorCount][11] = {
    /* slate */ {"#f8fafc","#f1f5f9","#e2e8f0","#cbd5e1","#94a3b8","#64748b","#475569","#334155","#1e293b","#0f172a","#020617"},
    /* gray */ {"#f9fafb","#f3f4f6","#e5e7eb","#d1d5db","#9ca3af","#6b7280","#4b5563","#374151","#1f2937","#111827","#030712"},
    /* zinc */ {"#fafafa","#f4f4f5","#e4e4e7","#d4d4d8","#a1a1aa","#71717a","#52525b","#3f3f46","#27272a","#18181b","#09090b"},
    /* neutral */ {"#fafafa","#f5f5f5","#e5e5e5","#d4d4d4","#a3a3a3","#737373","#525252","#404040","#262626","#171717","#0a0a0a"},
    /* stone */ {"#fafaf9","#f5f5f4","#e7e5e4","#d6d3d1","#a8a29e","#78716c","#57534e","#44403c","#292524","#1c1917","#0c0a09"},
    /* red */ {"#fef2f2","#fee2e2","#fecaca","#fca5a5","#f87171","#ef4444","#dc2626","#b91c1c","#991b1b","#7f1d1d","#450a0a"},
    /* orange */ {"#fff7ed","#ffedd5","#fed7aa","#fdba74","#fb923c","#f97316","#ea580c","#c2410c","#9a3412","#7c2d12","#431407"},
    /* amber */ {"#fffbeb","#fef3c7","#fde68a","#fcd34d","#fbbf24","#f59e0b","#d97706","#b45309","#92400e","#78350f","#451a03"},
    /* yellow */ {"#fefce8","#fef9c3","#fef08a","#fde047","#facc15","#eab308","#ca8a04","#a16207","#854d0e","#713f12","#422006"},
    /* lime */ {"#f7fee7","#ecfccb","#d9f99d","#bef264","#a3e635","#84cc16","#65a30d","#4d7c0f","#3f6212","#365314","#1a2e05"},
    /* green */ {"#f0fdf4","#dcfce7","#bbf7d0","#86efac","#4ade80","#22c55e","#16a34a","#15803d","#166534","#14532d","#052e16"},
    /* emerald */ {"#ecfdf5","#d1fae5","#a7f3d0","#6ee7b7","#34d399","#10b981","#059669","#047857","#065f46","#064e3b","#022c22"},
    /* teal */ {"#f0fdfa","#ccfbf1","#99f6e4","#5eead4","#2dd4bf","#14b8a6","#0d9488","#0f766e","#115e59","#134e4a","#042f2e"},
    /* cyan */ {"#ecfeff","#cffafe","#a5f3fc","#67e8f9","#22d3ee","#06b6d4","#0891b2","#0e7490","#155e75","#164e63","#083344"},
    /* sky */ {"#f0f9ff","#e0f2fe","#bae6fd","#7dd3fc","#38bdf8","#0ea5e9","#0284c7","#0369a1","#075985","#0c4a6e","#082f49"},
    /* blue */ {"#eff6ff","#dbeafe","#bfdbfe","#93c5fd","#60a5fa","#3b82f6","#2563eb","#1d4ed8","#1e40af","#1e3a8a","#172554"},
    /* indigo */ {"#eef2ff","#e0e7ff","#c7d2fe","#a5b4fc","#818cf8","#6366f1","#4f46e5","#4338ca","#3730a3","#312e81","#1e1b4b"},
    /* violet */ {"#f5f3ff","#ede9fe","#ddd6fe","#c4b5fd","#a78bfa","#8b5cf6","#7c3aed","#6d28d9","#5b21b6","#4c1d95","#2e1065"},
    /* purple */ {"#faf5ff","#f3e8ff","#e9d5ff","#d8b4fe","#c084fc","#a855f7","#9333ea","#7e22ce","#6b21a8","#581c87","#3b0764"},
    /* fuchsia */ {"#fdf4ff","#fae8ff","#f5d0fe","#f0abfc","#e879f9","#d946ef","#c026d3","#a21caf","#86198f","#701a75","#4a044e"},
    /* pink */ {"#fdf2f8","#fce7f3","#fbcfe8","#f9a8d4","#f472b6","#ec4899","#db2777","#be185d","#9d174d","#831843","#500724"},
    /* rose */ {"#fff1f2","#ffe4e6","#fecdd3","#fda4af","#fb7185","#f43f5e","#e11d48","#be123c","#9f1239","#881337","#4c0519"},
};

static const char *twHex(TwColor c, i32 shade) {
    if ((u32)c >= TwColorCount) return "#000000";
    i32 idx = shade == 50 ? 0 : shade == 950 ? 10 : shade / 100;
    if (idx < 0) idx = 0;
    if (idx > 10) idx = 10;
    return gc_tw_palette[c][idx];
}

static Node twBg(TwColor c, i32 shade) { return gc_tw("background-color", twHex(c, shade)); }
static Node twTextColor(TwColor c, i32 shade) { return gc_tw("color", twHex(c, shade)); }
static Node twBorderColor(TwColor c, i32 shade) { return gc_tw("border-color", twHex(c, shade)); }
#define TwBgWhite gc_tw("background-color", "#ffffff")
#define TwBgBlack gc_tw("background-color", "#000000")
#define TwTextWhite gc_tw("color", "#ffffff")
#define TwTextBlack gc_tw("color", "#000000")
#define TwBgTransparent gc_tw("background-color", "transparent")

/* --------------------------------------------------------------- spacing
 * n is the Tailwind scale: n * 0.25rem (twP(4) == p-4 == 1rem). */

static Node gc_tw_rem(const char *prop, i32 n) { return gc_tw(prop, gc_rem(n)); }
static Node gc_tw2_rem(const char *p1, const char *p2, i32 n) {
    const char *v = gc_rem(n);
    return gc_group2(gc_tw(p1, v), gc_tw(p2, v));
}

static Node twP(i32 n) { return gc_tw_rem("padding", n); }
static Node twPx(i32 n) { return gc_tw2_rem("padding-left", "padding-right", n); }
static Node twPy(i32 n) { return gc_tw2_rem("padding-top", "padding-bottom", n); }
static Node twPt(i32 n) { return gc_tw_rem("padding-top", n); }
static Node twPr(i32 n) { return gc_tw_rem("padding-right", n); }
static Node twPb(i32 n) { return gc_tw_rem("padding-bottom", n); }
static Node twPl(i32 n) { return gc_tw_rem("padding-left", n); }
static Node twM(i32 n) { return gc_tw_rem("margin", n); }
static Node twMx(i32 n) { return gc_tw2_rem("margin-left", "margin-right", n); }
static Node twMy(i32 n) { return gc_tw2_rem("margin-top", "margin-bottom", n); }
static Node twMt(i32 n) { return gc_tw_rem("margin-top", n); }
static Node twMr(i32 n) { return gc_tw_rem("margin-right", n); }
static Node twMb(i32 n) { return gc_tw_rem("margin-bottom", n); }
static Node twMl(i32 n) { return gc_tw_rem("margin-left", n); }
#define TwMxAuto gc_group2(gc_tw("margin-left", "auto"), gc_tw("margin-right", "auto"))
static Node twGap(i32 n) { return gc_tw_rem("gap", n); }
static Node twGapX(i32 n) { return gc_tw_rem("column-gap", n); }
static Node twGapY(i32 n) { return gc_tw_rem("row-gap", n); }

static Node twW(i32 n) { return gc_tw_rem("width", n); }
static Node twH(i32 n) { return gc_tw_rem("height", n); }
static Node twSize(i32 n) { return gc_group2(twW(n), twH(n)); }
static Node twMinW(i32 n) { return gc_tw_rem("min-width", n); }
static Node twMaxW(i32 n) { return gc_tw_rem("max-width", n); }
static Node twMinH(i32 n) { return gc_tw_rem("min-height", n); }
static Node twMaxH(i32 n) { return gc_tw_rem("max-height", n); }
static Node twWPercent(i32 pct) {
    char buf[16]; char *p = gc_fmt_i32(buf, pct); *p++ = '%'; *p = 0;
    return gc_tw("width", gc_strdup(buf, (u32)(p - buf)));
}
static Node twHPercent(i32 pct) {
    char buf[16]; char *p = gc_fmt_i32(buf, pct); *p++ = '%'; *p = 0;
    return gc_tw("height", gc_strdup(buf, (u32)(p - buf)));
}
#define TwWFull gc_tw("width", "100%")
#define TwHFull gc_tw("height", "100%")
#define TwWScreen gc_tw("width", "100vw")
#define TwHScreen gc_tw("height", "100vh")

/* ------------------------------------------------------ display + position */

#define TwBlock gc_tw("display", "block")
#define TwInlineBlock gc_tw("display", "inline-block")
#define TwInline gc_tw("display", "inline")
#define TwFlex gc_tw("display", "flex")
#define TwInlineFlex gc_tw("display", "inline-flex")
#define TwGrid gc_tw("display", "grid")
#define TwHidden gc_tw("display", "none")

#define TwStatic gc_tw("position", "static")
#define TwRelative gc_tw("position", "relative")
#define TwAbsolute gc_tw("position", "absolute")
#define TwFixed gc_tw("position", "fixed")
#define TwSticky gc_tw("position", "sticky")
static Node twTop(i32 n) { return gc_tw_rem("top", n); }
static Node twRight(i32 n) { return gc_tw_rem("right", n); }
static Node twBottom(i32 n) { return gc_tw_rem("bottom", n); }
static Node twLeft(i32 n) { return gc_tw_rem("left", n); }
static Node twInset(i32 n) { return gc_group2(gc_group2(twTop(n), twBottom(n)), gc_group2(twLeft(n), twRight(n))); }
static Node twZ(i32 n) {
    char buf[16]; char *p = gc_fmt_i32(buf, n); *p = 0;
    return gc_tw("z-index", gc_strdup(buf, (u32)(p - buf)));
}

#define TwOverflowHidden gc_tw("overflow", "hidden")
#define TwOverflowAuto gc_tw("overflow", "auto")
#define TwOverflowScroll gc_tw("overflow", "scroll")
#define TwOverflowYAuto gc_tw("overflow-y", "auto")
#define TwOverflowXAuto gc_tw("overflow-x", "auto")

/* ------------------------------------------------------------ flex + grid */

#define TwFlexRow gc_tw("flex-direction", "row")
#define TwFlexCol gc_tw("flex-direction", "column")
#define TwFlexWrap gc_tw("flex-wrap", "wrap")
#define TwFlex1 gc_tw("flex", "1 1 0%")
#define TwFlexAuto gc_tw("flex", "1 1 auto")
#define TwFlexNone gc_tw("flex", "none")
#define TwGrow gc_tw("flex-grow", "1")
#define TwGrow0 gc_tw("flex-grow", "0")
#define TwShrink0 gc_tw("flex-shrink", "0")
static Node twBasis(i32 n) { return gc_tw_rem("flex-basis", n); }

#define TwJustifyStart gc_tw("justify-content", "flex-start")
#define TwJustifyCenter gc_tw("justify-content", "center")
#define TwJustifyEnd gc_tw("justify-content", "flex-end")
#define TwJustifyBetween gc_tw("justify-content", "space-between")
#define TwJustifyAround gc_tw("justify-content", "space-around")
#define TwJustifyEvenly gc_tw("justify-content", "space-evenly")
#define TwItemsStart gc_tw("align-items", "flex-start")
#define TwItemsCenter gc_tw("align-items", "center")
#define TwItemsEnd gc_tw("align-items", "flex-end")
#define TwItemsStretch gc_tw("align-items", "stretch")
#define TwItemsBaseline gc_tw("align-items", "baseline")
#define TwSelfStart gc_tw("align-self", "flex-start")
#define TwSelfCenter gc_tw("align-self", "center")
#define TwSelfEnd gc_tw("align-self", "flex-end")

static Node twCols(i32 n) {
    char buf[48]; char *p = gwb_append_str(buf, "repeat(");
    p = gc_fmt_i32(p, n); p = gwb_append_str(p, ", minmax(0, 1fr))"); *p = 0;
    return gc_tw("grid-template-columns", gc_strdup(buf, gwb_strlen(buf)));
}
static Node twRows(i32 n) {
    char buf[48]; char *p = gwb_append_str(buf, "repeat(");
    p = gc_fmt_i32(p, n); p = gwb_append_str(p, ", minmax(0, 1fr))"); *p = 0;
    return gc_tw("grid-template-rows", gc_strdup(buf, gwb_strlen(buf)));
}
static Node twColSpan(i32 n) {
    char buf[32]; char *p = gwb_append_str(buf, "span ");
    p = gc_fmt_i32(p, n); p = gwb_append_str(p, " / span "); p = gc_fmt_i32(p, n); *p = 0;
    return gc_tw("grid-column", gc_strdup(buf, gwb_strlen(buf)));
}

/* -------------------------------------------------------------- typography */

typedef enum {
    TwTextXs, TwTextSm, TwTextBase, TwTextLg, TwTextXl,
    TwText2xl, TwText3xl, TwText4xl, TwText5xl, TwText6xl,
} TwTextScale;

/* Tailwind text sizes set BOTH font-size and line-height. */
static Node twTextSize(TwTextScale s) {
    static const char *fs[] = {"0.75rem","0.875rem","1rem","1.125rem","1.25rem","1.5rem","1.875rem","2.25rem","3rem","3.75rem"};
    static const char *lh[] = {"1rem","1.25rem","1.5rem","1.75rem","1.75rem","2rem","2.25rem","2.5rem","1","1"};
    return gc_group2(gc_tw("font-size", fs[s]), gc_tw("line-height", lh[s]));
}

static Node twWeight(i32 w) { /* 100..900 */
    char buf[8]; char *p = gc_fmt_i32(buf, w); *p = 0;
    return gc_tw("font-weight", gc_strdup(buf, gwb_strlen(buf)));
}
#define TwFontNormal twWeight(400)
#define TwFontMedium twWeight(500)
#define TwFontSemibold twWeight(600)
#define TwFontBold twWeight(700)
#define TwItalic gc_tw("font-style", "italic")
#define TwUppercase gc_tw("text-transform", "uppercase")
#define TwLowercase gc_tw("text-transform", "lowercase")
#define TwCapitalize gc_tw("text-transform", "capitalize")
#define TwUnderline gc_tw("text-decoration", "underline")
#define TwLineThrough gc_tw("text-decoration", "line-through")
#define TwNoUnderline gc_tw("text-decoration", "none")
#define TwTextLeft gc_tw("text-align", "left")
#define TwTextCenter gc_tw("text-align", "center")
#define TwTextRight gc_tw("text-align", "right")
#define TwTrackingTight gc_tw("letter-spacing", "-0.025em")
#define TwTrackingNormal gc_tw("letter-spacing", "0")
#define TwTrackingWide gc_tw("letter-spacing", "0.025em")
static Node twLeading(i32 n) { return gc_tw_rem("line-height", n); }
#define TwTruncate gc_group2(gc_group2( \
    gc_tw("overflow", "hidden"), gc_tw("text-overflow", "ellipsis")), \
    gc_tw("white-space", "nowrap"))
#define TwWhitespaceNowrap gc_tw("white-space", "nowrap")
#define TwWhitespacePre gc_tw("white-space", "pre")
#define TwFontMono gc_tw("font-family", "Consolas, 'Cascadia Mono', monospace")
#define TwFontSans gc_tw("font-family", "'Segoe UI', system-ui, sans-serif")

/* ---------------------------------------------------- borders + effects */

typedef enum {
    TwRoundedNone, TwRoundedSm, TwRounded, TwRoundedMd, TwRoundedLg,
    TwRoundedXl, TwRounded2xl, TwRounded3xl, TwRoundedFull,
} TwRoundedScale;

static Node twRounded(TwRoundedScale r) {
    static const char *v[] = {"0","0.125rem","0.25rem","0.375rem","0.5rem","0.75rem","1rem","1.5rem","9999px"};
    return gc_tw("border-radius", v[r]);
}
static Node twBorder(i32 px) {
    char buf[8]; char *p = gc_fmt_i32(buf, px); *p++ = 'p'; *p++ = 'x'; *p = 0;
    return gc_tw("border-width", gc_strdup(buf, gwb_strlen(buf)));
}
#define TwBorder gc_group2(twBorder(1), gc_tw("border-style", "solid"))
#define TwBorderSolid gc_tw("border-style", "solid")
/* Per-side borders (border-t/-r/-b/-l). Preflight sets border:0 solid, so a
 * width alone is enough; color via twBorderColor. Needed e.g. to style <hr>
 * (the preflight zeroes its native border, exactly like real Tailwind). */
static Node gc_tw_bside(const char *prop, i32 px) {
    char buf[8]; char *p = gc_fmt_i32(buf, px); *p++ = 'p'; *p++ = 'x'; *p = 0;
    return gc_tw(prop, gc_strdup(buf, gwb_strlen(buf)));
}
static Node twBorderT(i32 px) { return gc_tw_bside("border-top-width", px); }
static Node twBorderR(i32 px) { return gc_tw_bside("border-right-width", px); }
static Node twBorderB(i32 px) { return gc_tw_bside("border-bottom-width", px); }
static Node twBorderL(i32 px) { return gc_tw_bside("border-left-width", px); }
/* Per-side border color (accent bars etc.) */
static Node twBorderLColor(TwColor c, i32 shade) {
    return gc_tw("border-left-color", twHex(c, shade));
}
static Node twBorderTColor(TwColor c, i32 shade) {
    return gc_tw("border-top-color", twHex(c, shade));
}

typedef enum {
    TwShadowNone, TwShadowSm, TwShadow, TwShadowMd, TwShadowLg, TwShadowXl, TwShadow2xl,
} TwShadowScale;

static Node twShadow(TwShadowScale s) {
    static const char *v[] = {
        "none",
        "0 1px 2px 0 rgba(0,0,0,0.05)",
        "0 1px 3px 0 rgba(0,0,0,0.1), 0 1px 2px -1px rgba(0,0,0,0.1)",
        "0 4px 6px -1px rgba(0,0,0,0.1), 0 2px 4px -2px rgba(0,0,0,0.1)",
        "0 10px 15px -3px rgba(0,0,0,0.1), 0 4px 6px -4px rgba(0,0,0,0.1)",
        "0 20px 25px -5px rgba(0,0,0,0.1), 0 8px 10px -6px rgba(0,0,0,0.1)",
        "0 25px 50px -12px rgba(0,0,0,0.25)",
    };
    return gc_tw("box-shadow", v[s]);
}

static Node twOpacity(i32 pct) { /* 0..100 */
    char buf[16];
    char *p = buf;
    if (pct >= 100) { p = gwb_append_str(p, "1"); }
    else { p = gwb_append_str(p, "0."); if (pct < 10) *p++ = '0'; p = gc_fmt_i32(p, pct); }
    *p = 0;
    return gc_tw("opacity", gc_strdup(buf, gwb_strlen(buf)));
}

#define TwCursorPointer gc_tw("cursor", "pointer")
#define TwCursorDefault gc_tw("cursor", "default")
#define TwSelectNone gc_tw("user-select", "none")

/* ------------------------------------------------------------ transitions */

#define TwTransition gc_group2(gc_group2( \
    gc_tw("transition-property", "color, background-color, border-color, opacity, box-shadow, transform"), \
    gc_tw("transition-duration", "150ms")), \
    gc_tw("transition-timing-function", "cubic-bezier(0.4, 0, 0.2, 1)"))
static Node twDuration(i32 ms) {
    char buf[16]; char *p = gc_fmt_i32(buf, ms); *p++ = 'm'; *p++ = 's'; *p = 0;
    return gc_tw("transition-duration", gc_strdup(buf, gwb_strlen(buf)));
}
#define TwEaseLinear gc_tw("transition-timing-function", "linear")
#define TwEaseIn gc_tw("transition-timing-function", "cubic-bezier(0.4, 0, 1, 1)")
#define TwEaseOut gc_tw("transition-timing-function", "cubic-bezier(0, 0, 0.2, 1)")

#endif /* GWBC_TW_H */
