import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const [, , sourceDir, outputDir, platformLabel, version] = process.argv;

if (!sourceDir || !outputDir || !platformLabel || !version) {
  console.error(
    "Usage: node ./scripts/stage-release-assets.mjs <sourceDir> <outputDir> <platformLabel> <version>"
  );
  process.exit(1);
}

const allowedExtensions = [
  ".appimage",
  ".app",
  ".deb",
  ".dmg",
  ".exe",
  ".ipa",
  ".msi",
  ".rpm",
  ".sig",
  ".tar.gz",
  ".zip",
  ".apk",
  ".aab",
];

const sanitize = (value) =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

const extensionFor = (filename) => {
  const normalized = filename.toLowerCase();
  const matched = allowedExtensions.find((extension) =>
    normalized.endsWith(extension)
  );

  return matched ?? path.extname(normalized);
};

const walkFiles = (dir) => {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walkFiles(fullPath));
      continue;
    }

    files.push(fullPath);
  }

  return files;
};

const walkAppBundles = (dir) => {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const bundles = [];

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory() && entry.name.toLowerCase().endsWith(".app")) {
      bundles.push(fullPath);
      continue;
    }

    if (entry.isDirectory()) {
      bundles.push(...walkAppBundles(fullPath));
    }
  }

  return bundles;
};

if (!fs.existsSync(sourceDir)) {
  console.error(`Source directory does not exist: ${sourceDir}`);
  process.exit(1);
}

fs.mkdirSync(outputDir, { recursive: true });

const sourceFiles = walkFiles(sourceDir).filter((file) => {
  const relativePath = path.relative(sourceDir, file).toLowerCase();
  return allowedExtensions
    .filter((extension) => extension !== ".app")
    .some((extension) => relativePath.endsWith(extension));
});
const appBundles = walkAppBundles(sourceDir);

if (sourceFiles.length === 0 && appBundles.length === 0) {
  console.error(`No releasable assets found in: ${sourceDir}`);
  process.exit(1);
}

for (const bundle of appBundles) {
  const baseName = path.basename(bundle);
  const extension = ".app";
  const stem = baseName.slice(0, Math.max(0, baseName.length - extension.length));
  const normalizedName = sanitize(stem);
  const targetFileName = `omninova-claw_${sanitize(version)}_${sanitize(platformLabel)}_${normalizedName}${extension}`;
  const targetPath = path.join(outputDir, targetFileName);
  fs.rmSync(targetPath, { recursive: true, force: true });
  fs.cpSync(bundle, targetPath, { recursive: true });
  console.log(`staged: ${targetFileName}`);
}

for (const file of sourceFiles) {
  const baseName = path.basename(file);
  const extension = extensionFor(baseName);
  const stem = baseName.slice(0, Math.max(0, baseName.length - extension.length));
  const normalizedName = sanitize(stem);
  const targetFileName = `omninova-claw_${sanitize(version)}_${sanitize(platformLabel)}_${normalizedName}${extension}`;
  const targetPath = path.join(outputDir, targetFileName);
  fs.copyFileSync(file, targetPath);
  console.log(`staged: ${targetFileName}`);
}
