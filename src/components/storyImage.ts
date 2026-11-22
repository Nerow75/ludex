// Image d'une page de la story : PNG 1080 × 1920 dessiné dans un canvas, avec les mêmes proportions
// que l'écran (1 cqw de `.story__frame` = 10,8 px ici) et les couleurs lues dans les tokens `--lx-*`.
// Images chargées en `crossOrigin = "anonymous"` : celles du protocole `ludex` (en-tête CORS) se
// dessinent, une image qui refuse CORS ne se charge pas du tout et la page est dessinée sans elle
// (jamais de canvas « tainté »). Rust reçoit seulement les octets (`save_story_image`).
import type { Game } from "../lib/api";
import { titleColor } from "../lib/color";
import { t } from "../lib/i18n";
import { rarityText, type StoryPage } from "../lib/story";

export const STORY_WIDTH = 1080;
export const STORY_HEIGHT = 1920;
/** 1 cqw du cadre affiché, en pixels de l'image. */
const U = STORY_WIDTH / 100;
const PAD = 72;
const BOTTOM = 140;
const CONTENT = STORY_WIDTH - 2 * PAD;
const GAP = 24;
/** Une image qui ne répond pas ne bloque pas l'enregistrement. */
const IMAGE_TIMEOUT_MS = 5000;

type Ctx = CanvasRenderingContext2D;

const token = (name: string, fallback: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

function loadImage(url: string | null | undefined): Promise<HTMLImageElement | null> {
  if (!url) return Promise.resolve(null);
  return new Promise((resolve) => {
    const img = new Image();
    const timer = window.setTimeout(() => resolve(null), IMAGE_TIMEOUT_MS);
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.onload = () => {
      window.clearTimeout(timer);
      resolve(img.naturalWidth > 0 ? img : null);
    };
    img.onerror = () => {
      window.clearTimeout(timer);
      resolve(null);
    };
    img.src = url;
  });
}

/** Couleur CSS quelconque (token, hsl…) avec une opacité, normalisée par le canvas lui-même. */
function alpha(ctx: Ctx, color: string, a: number): string {
  ctx.fillStyle = "#000000";
  ctx.fillStyle = color;
  const c = String(ctx.fillStyle);
  if (c.startsWith("#") && c.length === 7) {
    const n = parseInt(c.slice(1), 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
  }
  const m = c.match(/rgba?\(([^)]+)\)/);
  if (!m) return c;
  const [r, g, b, base = "1"] = m[1].split(",").map((s) => s.trim());
  return `rgba(${r}, ${g}, ${b}, ${Number(base) * a})`;
}

/** Image recadrée au centre pour remplir le rectangle (comme `object-fit: cover`). */
function coverFit(ctx: Ctx, img: HTMLImageElement, x: number, y: number, w: number, h: number) {
  const scale = Math.max(w / img.naturalWidth, h / img.naturalHeight);
  const sw = w / scale;
  const sh = h / scale;
  ctx.drawImage(img, (img.naturalWidth - sw) / 2, (img.naturalHeight - sh) / 2, sw, sh, x, y, w, h);
}

/** Lignes d'un texte coupé aux mots, `max` lignes au plus (la dernière finie par « … »). */
function wrap(ctx: Ctx, text: string, width: number, max: number): string[] {
  const words = text.split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let line = "";
  for (const word of words) {
    const tryLine = line ? `${line} ${word}` : word;
    if (ctx.measureText(tryLine).width <= width || !line) line = tryLine;
    else {
      lines.push(line);
      line = word;
    }
  }
  if (line) lines.push(line);
  if (lines.length <= max) return lines.map((l) => ellipsis(ctx, l, width));
  const kept = lines.slice(0, max);
  kept[max - 1] = ellipsis(ctx, `${kept[max - 1]} ${lines.slice(max).join(" ")}`, width);
  return kept;
}

function ellipsis(ctx: Ctx, text: string, width: number): string {
  if (ctx.measureText(text).width <= width) return text;
  let t = text;
  while (t.length > 1 && ctx.measureText(`${t}…`).width > width) t = t.slice(0, -1);
  return `${t.trimEnd()}…`;
}

interface Block {
  height: number;
  draw: (y: number) => void;
}

export interface StoryColors {
  base: string;
  deep: string;
  onArt: string;
  onArt2: string;
  scrim: string;
  badge: string;
  accent: string;
  accentDeep: string;
  onAccent: string;
  body: string;
  mono: string;
}

const colors = (): StoryColors => ({
  base: token("--lx-art-base", "#0B0B0B"),
  deep: token("--lx-art-deep", "#050505"),
  onArt: token("--lx-on-art", "#FFFFFF"),
  onArt2: token("--lx-on-art-2", "rgba(255, 255, 255, 0.75)"),
  scrim: token("--lx-art-scrim", "rgba(0, 0, 0, 0.78)"),
  badge: token("--lx-art-badge", "rgba(0, 0, 0, 0.7)"),
  accent: token("--lx-accent", "#14B8A6"),
  accentDeep: token("--lx-accent-deep", "#0E8F81"),
  onAccent: token("--lx-on-accent", "#0B0B0B"),
  body: token("--lx-font-body", "sans-serif"),
  mono: token("--lx-font-mono", "monospace"),
});

/** Logo de Ludex (même dessin que `LudexMark`, boîte de 64). */
function drawMark(ctx: Ctx, c: StoryColors, x: number, y: number, size: number) {
  const k = size / 64;
  ctx.save();
  ctx.translate(x, y);
  ctx.scale(k, k);
  ctx.fillStyle = c.accentDeep;
  ctx.beginPath();
  ctx.roundRect(2, 2, 60, 60, 14);
  ctx.fill();
  ctx.fillStyle = c.accent;
  ctx.beginPath();
  ctx.roundRect(7, 5, 50, 46, 10);
  ctx.fill();
  const play = new Path2D("M37 11.5v17l14-8.5z");
  ctx.fillStyle = c.onAccent;
  ctx.strokeStyle = c.onAccent;
  ctx.lineWidth = 1.5;
  ctx.lineJoin = "round";
  ctx.fill(play);
  ctx.stroke(play);
  ctx.restore();
}

/** Dessine une page et renvoie les octets du PNG. */
export async function drawStoryPage(page: StoryPage, byId: Map<string, Game>): Promise<Uint8Array> {
  const c = colors();
  const game = page.gameId ? byId.get(page.gameId) : undefined;
  const itemGames = page.items.map((i) => byId.get(i.gameId));
  // Polices de l'interface chargées avant de mesurer le texte.
  await Promise.all(
    ["400", "500", "600"].map((w) => document.fonts.load(`${w} 40px ${c.body}`).catch(() => [])).concat([document.fonts.load(`500 38px ${c.mono}`).catch(() => [])]),
  );
  const [hero, cover, logo, icon, ...covers] = await Promise.all([
    loadImage(game?.heroUrl),
    game?.heroUrl ? Promise.resolve(null) : loadImage(game?.coverUrl),
    page.logo ? loadImage(game?.logoUrl) : Promise.resolve(null),
    loadImage(page.achievement?.icon),
    ...itemGames.map((g) => loadImage(g?.coverUrl)),
  ]);

  const canvas = document.createElement("canvas");
  canvas.width = STORY_WIDTH;
  canvas.height = STORY_HEIGHT;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error(t.activity.story.canvasUnavailable);
  const W = STORY_WIDTH;
  const H = STORY_HEIGHT;

  // Fond : image large du jeu, sinon sa jaquette floutée, sinon la teinte du titre.
  ctx.fillStyle = c.base;
  ctx.fillRect(0, 0, W, H);
  if (hero) {
    ctx.filter = "brightness(0.62) saturate(1.1)";
    coverFit(ctx, hero, 0, 0, W, H);
  } else if (cover) {
    ctx.filter = "blur(44px) brightness(0.5) saturate(1.2)";
    coverFit(ctx, cover, -120, -120, W + 240, H + 240);
  } else {
    const tint = titleColor(game?.title ?? page.title);
    const g = ctx.createRadialGradient(W / 2, H * 0.38, 0, W / 2, H * 0.38, H * 0.75);
    g.addColorStop(0, alpha(ctx, tint, 0.5));
    g.addColorStop(1, alpha(ctx, c.deep, 1));
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, W, H);
  }
  ctx.filter = "none";

  // Voiles : léger en haut (barre de la marque), plein en bas sous le texte.
  const top = ctx.createLinearGradient(0, 0, 0, H * 0.22);
  top.addColorStop(0, alpha(ctx, c.base, 0.6));
  top.addColorStop(1, alpha(ctx, c.base, 0));
  ctx.fillStyle = top;
  ctx.fillRect(0, 0, W, H * 0.22);
  const bottom = ctx.createLinearGradient(0, H * 0.25, 0, H);
  bottom.addColorStop(0, alpha(ctx, c.base, 0));
  bottom.addColorStop(0.45, alpha(ctx, c.scrim, 1));
  bottom.addColorStop(1, alpha(ctx, c.base, 0.96));
  ctx.fillStyle = bottom;
  ctx.fillRect(0, H * 0.25, W, H * 0.75);

  // Marque en haut.
  ctx.textBaseline = "middle";
  drawMark(ctx, c, PAD, PAD, 64);
  ctx.fillStyle = c.onArt;
  ctx.font = `600 ${44}px ${c.body}`;
  ctx.fillText("Ludex", PAD + 64 + 20, PAD + 32);
  ctx.fillStyle = c.onArt2;
  ctx.font = `500 ${36}px ${c.body}`;
  ctx.textAlign = "right";
  ctx.fillText(t.activity.story.brand, W - PAD, PAD + 32);
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";

  // Contenu, ancré en bas : blocs mesurés puis dessinés de haut en bas.
  const blocks: Block[] = [];
  const text = (font: string, color: string, lines: string[], size: number, lineHeight: number, spacing = "0px") => {
    blocks.push({
      height: lines.length * size * lineHeight,
      draw: (y) => {
        ctx.font = font;
        ctx.fillStyle = color;
        ctx.letterSpacing = spacing;
        lines.forEach((l, i) => ctx.fillText(l, PAD, y + size * (lineHeight * i + (lineHeight + 0.72) / 2)));
        ctx.letterSpacing = "0px";
      },
    });
  };

  if (logo) {
    const scale = Math.min((80 * U) / logo.naturalWidth, (20 * U) / logo.naturalHeight);
    const w = logo.naturalWidth * scale;
    const h = logo.naturalHeight * scale;
    blocks.push({ height: h, draw: (y) => ctx.drawImage(logo, PAD, y, w, h) });
  }
  ctx.font = `500 40px ${c.body}`;
  text(ctx.font, c.onArt2, wrap(ctx, page.eyebrow, CONTENT, 2), 40, 1.3);
  if (page.value) {
    let size = 184;
    ctx.letterSpacing = `${-0.04 * size}px`;
    ctx.font = `600 ${size}px ${c.body}`;
    const width = ctx.measureText(page.value).width;
    if (width > CONTENT) size = Math.floor((size * CONTENT) / width);
    ctx.letterSpacing = "0px";
    text(`600 ${size}px ${c.body}`, c.onArt, [page.value], size, 1.05, `${-0.04 * size}px`);
  }
  ctx.font = `600 68px ${c.body}`;
  text(ctx.font, c.onArt, wrap(ctx, page.title, CONTENT, 3), 68, 1.15);
  if (page.detail) {
    ctx.font = `400 40px ${c.body}`;
    text(ctx.font, c.onArt2, wrap(ctx, page.detail, CONTENT, 3), 40, 1.35);
  }

  if (page.items.length) {
    const row = 108;
    const gap = 28;
    blocks.push({
      height: page.items.length * row + (page.items.length - 1) * gap,
      draw: (y0) => {
        // Colonne des valeurs à la largeur de la plus longue : barres de même longueur, comme à l'écran.
        ctx.font = `500 38px ${c.mono}`;
        const valueW = Math.max(...page.items.map((item) => ctx.measureText(item.value).width), STORY_WIDTH * 0.11);
        page.items.forEach((item, i) => {
          const y = y0 + i * (row + gap);
          const img = covers[i];
          ctx.save();
          ctx.beginPath();
          ctx.roundRect(PAD, y, 72, row, 8);
          ctx.clip();
          if (img) coverFit(ctx, img, PAD, y, 72, row);
          else {
            ctx.fillStyle = alpha(ctx, titleColor(item.label), 0.55);
            ctx.fillRect(PAD, y, 72, row);
          }
          ctx.restore();
          ctx.font = `500 38px ${c.mono}`;
          ctx.fillStyle = c.onArt2;
          ctx.textAlign = "right";
          ctx.fillText(item.value, W - PAD, y + row / 2 + 13);
          ctx.textAlign = "left";
          const left = PAD + 72 + 28;
          const labelW = W - PAD - valueW - 28 - left;
          ctx.font = `500 44px ${c.body}`;
          ctx.fillStyle = c.onArt;
          const labelY = item.share > 0 ? y + row / 2 - 4 : y + row / 2 + 15;
          ctx.fillText(ellipsis(ctx, item.label, labelW), left, labelY);
          if (item.share > 0) {
            ctx.fillStyle = alpha(ctx, c.onArt, 0.18);
            ctx.beginPath();
            ctx.roundRect(left, y + row / 2 + 20, labelW, 10, 5);
            ctx.fill();
            ctx.fillStyle = c.accent;
            ctx.beginPath();
            ctx.roundRect(left, y + row / 2 + 20, Math.max(10, labelW * item.share), 10, 5);
            ctx.fill();
          }
        });
      },
    });
  }

  if (page.bars.length) {
    const height = 30 * U;
    const label = 36;
    const n = page.bars.length;
    const col = (CONTENT - (n - 1) * GAP) / n;
    blocks.push({
      height,
      draw: (y) => {
        const track = height - label - 16;
        page.bars.forEach((b, i) => {
          const x = PAD + i * (col + GAP);
          ctx.fillStyle = alpha(ctx, c.onArt, 0.12);
          ctx.beginPath();
          ctx.roundRect(x, y, col, track, 10);
          ctx.fill();
          const h = Math.max(track * 0.03, track * b.share);
          ctx.fillStyle = b.active ? c.accent : alpha(ctx, c.onArt, 0.45);
          ctx.beginPath();
          ctx.roundRect(x, y + track - h, col, h, 10);
          ctx.fill();
          ctx.font = `${b.active ? 600 : 500} ${label}px ${c.body}`;
          ctx.fillStyle = b.active ? c.onArt : c.onArt2;
          ctx.textAlign = "center";
          ctx.fillText(b.label, x + col / 2, y + height - 6);
          ctx.textAlign = "left";
        });
      },
    });
  }

  const rare = page.achievement;
  if (rare) {
    const pad = 40;
    const iconSize = 128;
    const left = PAD + pad + iconSize + 36;
    const width = W - PAD - pad - left;
    const gameTitle = byId.get(rare.gameId)?.title;
    const lines: { font: string; color: string; text: string; size: number }[] = [
      { font: `500 32px ${c.body}`, color: c.accent, text: t.activity.story.rarest, size: 32 },
      { font: `600 46px ${c.body}`, color: c.onArt, text: rare.name, size: 46 },
      { font: `400 34px ${c.body}`, color: c.onArt2, text: rarityText(rare.percent), size: 34 },
      ...(gameTitle ? [{ font: `400 34px ${c.body}`, color: c.onArt2, text: gameTitle, size: 34 }] : []),
    ];
    const textH = lines.reduce((h, l) => h + l.size * 1.3, 0);
    const inner = Math.max(iconSize, textH);
    blocks.push({
      height: inner + 2 * pad,
      draw: (y) => {
        ctx.fillStyle = c.badge;
        ctx.beginPath();
        ctx.roundRect(PAD, y, CONTENT, inner + 2 * pad, 24);
        ctx.fill();
        // Icône, ou cadre vide comme à l'écran (icône du CDN sans CORS : jamais dans le canvas).
        ctx.save();
        ctx.beginPath();
        ctx.roundRect(PAD + pad, y + pad + (inner - iconSize) / 2, iconSize, iconSize, 16);
        ctx.clip();
        if (icon) ctx.drawImage(icon, PAD + pad, y + pad + (inner - iconSize) / 2, iconSize, iconSize);
        else {
          ctx.fillStyle = alpha(ctx, c.onArt, 0.12);
          ctx.fillRect(PAD + pad, y + pad + (inner - iconSize) / 2, iconSize, iconSize);
        }
        ctx.restore();
        let ty = y + pad + (inner - textH) / 2;
        for (const l of lines) {
          ctx.font = l.font;
          ctx.fillStyle = l.color;
          ty += l.size * 1.3;
          ctx.fillText(ellipsis(ctx, l.text, width), left, ty - l.size * 0.3);
        }
      },
    });
  }

  const total = blocks.reduce((h, b) => h + b.height, 0) + GAP * (blocks.length - 1);
  let y = Math.max(PAD + 64 + 48, H - BOTTOM - total);
  for (const b of blocks) {
    b.draw(y);
    y += b.height + GAP;
  }

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error(t.activity.story.imageFailed);
  return new Uint8Array(await blob.arrayBuffer());
}
