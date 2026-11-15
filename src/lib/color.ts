/** Couleur stable dérivée du titre, utilisée quand aucune jaquette n'est lisible. */
export function titleColor(title: string): string {
  let h = 0;
  for (const c of title) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `hsl(${h % 360} 62% 64%)`;
}

/** Teinte dominante d'une image, éclaircie pour rester lisible sous un texte sombre. */
function dominant(img: HTMLImageElement): string | null {
  const size = 24;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, size, size);
  let data: Uint8ClampedArray;
  try {
    data = ctx.getImageData(0, 0, size, size).data; // lève une erreur si l'image n'autorise pas CORS
  } catch {
    return null;
  }

  let x = 0, y = 0, sat = 0, weight = 0;
  for (let i = 0; i < data.length; i += 4) {
    const r = data[i] / 255, g = data[i + 1] / 255, b = data[i + 2] / 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    const l = (max + min) / 2;
    const d = max - min;
    if (d < 0.08 || l < 0.12 || l > 0.92) continue;
    const s = d / (1 - Math.abs(2 * l - 1));
    let hue = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    hue = (hue * 60 + 360) % 360;
    const w = s * s;
    x += Math.cos((hue * Math.PI) / 180) * w;
    y += Math.sin((hue * Math.PI) / 180) * w;
    sat += s * w;
    weight += w;
  }
  if (weight < 0.5) return null;
  const hue = ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
  const s = Math.min(0.85, Math.max(0.5, sat / weight));
  return `hsl(${Math.round(hue)} ${Math.round(s * 100)}% 66%)`;
}

export function ambientFromImage(url: string): Promise<string | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(dominant(img));
    img.onerror = () => resolve(null);
    img.src = url;
  });
}
