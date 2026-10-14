const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;

export function validateVersion(next, current) {
  const a = SEMVER.exec(next ?? '');
  const b = SEMVER.exec(current);
  if (!a) throw new Error(`versiune invalidă: ${next} (format X.Y.Z)`);
  const cmp = [1, 2, 3].map((i) => Number(a[i]) - Number(b[i])).find((d) => d !== 0) ?? 0;
  if (cmp <= 0) throw new Error(`versiunea ${next} trebuie să fie mai mare decât ${current}`);
}

export function setVersion(fileName, text, version) {
  if (fileName.endsWith('.json')) {
    const obj = JSON.parse(text);
    obj.version = version;
    return JSON.stringify(obj, null, 2) + '\n';
  }
  // Cargo.toml: doar prima linie `version = "..."` (cea din [package])
  return text.replace(/^version = "[^"]*"/m, `version = "${version}"`);
}

// GitHub înlocuiește spațiile din numele fișierelor urcate, deci folosim un nume fără spații.
export function assetName(version) {
  return `DynamicIsland_${version}_x64-setup.exe`;
}

// Formatul `latest.json` citit de tauri-plugin-updater.
export function manifest({ version, notes, pubDate, signature, repo, baseUrl }) {
  const base = baseUrl ?? `https://github.com/${repo}/releases/download/v${version}`;
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: {
      'windows-x86_64': { signature, url: `${base}/${assetName(version)}` },
    },
  };
}
