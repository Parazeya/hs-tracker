// Capture failures must name the selected backend rather than recommend an
// unrelated Windows driver on Linux or when Packet Monitor was selected.
export function capturePermissionCommand(binary) {
  const path = binary || '<path/to/hs-tracker>';
  return `sudo setcap cap_net_raw=ep '${path.replaceAll("'", "'\\''")}'`;
}

export function captureBackendNotice(status, t = value => value, { onLinux = false, binary = '' } = {}) {
  if (status === 'waiting-for-game-connection') return {
    bad: false,
    title: t('Waiting for a game connection'),
    detail: t('Hero Siege is running. Capture will start when its TCP endpoints are known.'),
  };
  if (status?.startsWith('capture-error|')) {
    const [, backend, ...reason] = status.split('|');
    return { bad: true, title: t('Capture backend unavailable'), detail: `${backend}: ${reason.join('|')}` };
  }
  if (status === 'no-access' && onLinux) return {
    bad: true,
    title: t('Not allowed to read network traffic'),
    detail: t('Linux refused access to the capture interface. Check the capture capability and the package installation.'),
    fix: capturePermissionCommand(binary),
  };
  return null;
}
