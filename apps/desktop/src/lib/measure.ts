/** Width of a text in the lyric font, in CSS pixels. */
export type Measure = (text: string) => number;

const widths = new Map<string, number>();
let context: CanvasRenderingContext2D | null | undefined;
let family = "";

/** Measures lyric lines at `fontSize`, in the overlay's semibold font. Results are kept. */
export function lyricMeasure(fontSize: number): Measure {
  return (text) => {
    const key = `${fontSize}|${text}`;
    const known = widths.get(key);
    if (known !== undefined) return known;
    if (context === undefined) {
      context = typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
      family = typeof document === "undefined" ? "" : getComputedStyle(document.documentElement).fontFamily;
    }
    if (!context) return text.length * fontSize * 0.55;
    context.font = `600 ${fontSize}px ${family || "sans-serif"}`;
    const width = context.measureText(text).width;
    if (widths.size > 800) widths.clear();
    widths.set(key, width);
    return width;
  };
}
