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
    // package-lock.json: și pachetul rădăcină
    if (obj.packages?.['']) obj.packages[''].version = version;
    return JSON.stringify(obj, null, 2) + '\n';
  }
  // Cargo.toml: doar prima linie `version = "..."` (cea din [package])
  return text.replace(/^version = "[^"]*"/m, `version = "${version}"`);
}

// GitHub înlocuiește spațiile din numele fișierelor urcate, deci folosim un nume fără spații.
export function assetName(version) {
  return `DynamicIsland_${version}_x64-setup.exe`;
}

// Installerul produs de `tauri build`, în folderul de compilare al cargo (poate fi în afara repo-ului).
export function builtInstaller(targetDir, version) {
  return `${targetDir}/release/bundle/nsis/Dynamic Island_${version}_x64-setup.exe`;
}

export const VERSION_FILES = ['package.json', 'package-lock.json', 'src-tauri/Cargo.toml', 'src-tauri/tauri.conf.json'];

export function checkBranch(branch) {
  if (branch !== 'main') throw new Error(`release-ul se face doar de pe main (acum: ${branch})`);
}

// Ce rulezi ca să repari un release oprit la jumătate, în funcție de pasul care a eșuat.
export function recoveryHint(stage, version, repo) {
  const release = `gh release create v${version} dist-release/${assetName(version)} dist-release/latest.json --repo ${repo} --title v${version}`;
  if (stage === 'build') {
    return `Build-ul a eșuat. Readu fișierele de versiune:\n  git checkout -- ${VERSION_FILES.join(' ')} src-tauri/Cargo.lock`;
  }
  if (stage === 'push') {
    return `Commit-ul și tag-ul v${version} există local. Termină publicarea (fără rebuild):\n  git push --atomic origin main v${version}\n  ${release}`;
  }
  return `Codul și tag-ul sunt pe GitHub, dar release-ul nu e publicat. Dacă există ca ciornă:\n  gh release edit v${version} --draft=false --latest --repo ${repo}\naltfel creează-l:\n  ${release}`;
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
