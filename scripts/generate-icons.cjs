#!/usr/bin/env node
/**
 * Generates all Tauri icon assets from the SVG logo.
 * Run: node scripts/generate-icons.cjs
 *
 * Requires: npm install sharp
 */

const sharp = require('sharp');
const path = require('path');

const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 32 32" fill="none">
  <defs>
    <linearGradient id="sp_a" x1="0" x2="20.918" y1="30.391" y2="19.069" gradientUnits="userSpaceOnUse">
      <stop stop-color="#5236FF"/>
      <stop offset="1" stop-color="#D9EDFF"/>
    </linearGradient>
    <linearGradient id="sp_b" x1="18" x2="18" y1="1" y2="29.813" gradientUnits="userSpaceOnUse">
      <stop stop-color="#8587FF"/>
      <stop offset="1" stop-color="#383BC3"/>
    </linearGradient>
  </defs>
  <path d="M19.56 8.813 15.933 11 14.12 9.92l3.64-2.187-1.853-1.093-5.44 3.293 5.48 3.267L21.4 9.907l-1.84-1.094Z" fill="#000000"/>
  <path fill-rule="evenodd" clip-rule="evenodd" d="m14.667 14.867-.014-.014L4 8.333l-4 12.32L14.653 29.8l.014.013V14.867Z" fill="url(#sp_a)"/>
  <path fill-rule="evenodd" clip-rule="evenodd" d="M28 8.333 16 1 4 8.333l2.547 1.56L16 4.12l9.44 5.773-8.093 4.96-.014.014v14.946l.014-.013L32 20.653l-4-12.32Z" fill="url(#sp_b)"/>
</svg>`;

// Monochrome version for macOS menu bar tray
const TRAY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 32 32" fill="none">
  <path d="M19.56 8.813 15.933 11 14.12 9.92l3.64-2.187-1.853-1.093-5.44 3.293 5.48 3.267L21.4 9.907l-1.84-1.094Z" fill="#000000"/>
  <path fill-rule="evenodd" clip-rule="evenodd" d="m14.667 14.867-.014-.014L4 8.333l-4 12.32L14.653 29.8l.014.013V14.867Z" fill="#000000"/>
  <path fill-rule="evenodd" clip-rule="evenodd" d="M28 8.333 16 1 4 8.333l2.547 1.56L16 4.12l9.44 5.773-8.093 4.96-.014.014v14.946l.014-.013L32 20.653l-4-12.32Z" fill="#000000"/>
</svg>`;

function makeSvg(size, bgColor = null, paddingPercent = 0) {
  const p = Math.round(size * paddingPercent);
  const inner = size - p * 2;
  const radius = Math.round(inner * 0.22);
  const bg = bgColor
    ? `<rect x="${p}" y="${p}" width="${inner}" height="${inner}" rx="${radius}" ry="${radius}" fill="${bgColor}"/>`
    : '';
  const logoPad = Math.round(size * (paddingPercent + 0.08));
  const logoSize = size - logoPad * 2;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  ${bg}
  <svg x="${logoPad}" y="${logoPad}" width="${logoSize}" height="${logoSize}">
    ${ICON_SVG}
  </svg>
</svg>`;
}

async function generate() {
  const iconsDir = path.join(__dirname, '../src-tauri/icons');

  const pngTasks = [
    { file: '32x32.png',       size: 32,   bg: null,      padding: 0 },
    { file: '128x128.png',     size: 128,  bg: null,      padding: 0 },
    { file: '128x128@2x.png',  size: 256,  bg: null,      padding: 0 },
    { file: 'icon.png',        size: 1024, bg: '#FFFFFF',  padding: 0.10 },
    { file: 'Square30x30Logo.png',   size: 30,  bg: null, padding: 0 },
    { file: 'Square44x44Logo.png',   size: 44,  bg: null, padding: 0 },
    { file: 'Square71x71Logo.png',   size: 71,  bg: null, padding: 0 },
    { file: 'Square89x89Logo.png',   size: 89,  bg: null, padding: 0 },
    { file: 'Square107x107Logo.png', size: 107, bg: null, padding: 0 },
    { file: 'Square142x142Logo.png', size: 142, bg: null, padding: 0 },
    { file: 'Square150x150Logo.png', size: 150, bg: null, padding: 0 },
    { file: 'Square284x284Logo.png', size: 284, bg: null, padding: 0 },
    { file: 'Square310x310Logo.png', size: 310, bg: null, padding: 0 },
    { file: 'StoreLogo.png',         size: 50,  bg: null, padding: 0 },
  ];

  for (const { file, size, bg, padding } of pngTasks) {
    const svg = makeSvg(size, bg, padding);
    const outputPath = path.join(iconsDir, file);
    await sharp(Buffer.from(svg)).png().toFile(outputPath);
    console.log(`✓ ${file}`);
  }

  // Generate tray icon (monochrome, 64x64 for macOS menu bar)
  const traySvg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 32 32">${TRAY_SVG}</svg>`;
  await sharp(Buffer.from(traySvg)).resize(64, 64).png().toFile(path.join(iconsDir, 'tray-icon.png'));
  console.log('✓ tray-icon.png');

  // Generate icon.ico with multiple sizes (16, 32, 48, 64, 128, 256)
  const icoSizes = [16, 32, 48, 64, 128, 256];
  const icoBuffers = await Promise.all(
    icoSizes.map(size => sharp(Buffer.from(makeSvg(size, '#FFFFFF', 0.08))).resize(size, size).png().toBuffer())
  );

  // Build ICO file manually
  const numImages = icoSizes.length;
  const headerSize = 6 + numImages * 16;
  let offset = headerSize;
  const header = Buffer.alloc(headerSize);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(numImages, 4);
  for (let i = 0; i < numImages; i++) {
    const size = icoSizes[i];
    const buf = icoBuffers[i];
    header.writeUInt8(size === 256 ? 0 : size, 6 + i * 16);     // width
    header.writeUInt8(size === 256 ? 0 : size, 6 + i * 16 + 1); // height
    header.writeUInt8(0, 6 + i * 16 + 2);   // color count
    header.writeUInt8(0, 6 + i * 16 + 3);   // reserved
    header.writeUInt16LE(1, 6 + i * 16 + 4); // planes
    header.writeUInt16LE(32, 6 + i * 16 + 6); // bit count
    header.writeUInt32LE(buf.length, 6 + i * 16 + 8);  // size
    header.writeUInt32LE(offset, 6 + i * 16 + 12); // offset
    offset += buf.length;
  }
  const icoData = Buffer.concat([header, ...icoBuffers]);
  require('fs').writeFileSync(path.join(iconsDir, 'icon.ico'), icoData);
  console.log('✓ icon.ico (16, 32, 48, 64, 128, 256)');

  console.log('\nNote: icon.icns requires manual generation on macOS:');
  console.log('  Use the icon.png (1024x1024) with: pnpm tauri icon src-tauri/icons/icon.png');
}

generate().catch((err) => {
  console.error('Failed:', err.message);
  process.exit(1);
});
