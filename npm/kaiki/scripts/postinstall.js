const packages = {
  'darwin arm64': '@kaiki/cli-darwin-arm64',
  'darwin x64': '@kaiki/cli-darwin-x64',
  'linux x64 glibc': '@kaiki/cli-linux-x64-gnu',
  'linux x64 musl': '@kaiki/cli-linux-x64-musl',
  'linux arm64 glibc': '@kaiki/cli-linux-arm64-gnu',
  'linux arm64 musl': '@kaiki/cli-linux-arm64-musl',
  'win32 x64': '@kaiki/cli-win32-x64-msvc',
  'win32 arm64': '@kaiki/cli-win32-arm64-msvc',
};

function getExpectedPackage() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'linux') {
    const isMusl = !process.report.getReport().header.glibcVersionRuntime;
    const libc = isMusl ? 'musl' : 'glibc';
    return packages[`${platform} ${arch} ${libc}`];
  }

  return packages[`${platform} ${arch}`];
}

const expected = getExpectedPackage();

if (!expected) {
  console.warn(
    `kaiki: unsupported platform ${process.platform} ${process.arch}. ` +
      `Please open an issue at https://github.com/re-taro/kaiki/issues`,
  );
  process.exit(0);
}

try {
  require.resolve(expected);
} catch {
  console.warn(
    `kaiki: could not find the platform-specific binary package "${expected}".\n` +
      `If you are using npm, make sure optional dependencies are not disabled.\n` +
      `The CLI will not work until the correct platform package is installed.`,
  );
}

process.exit(0);
