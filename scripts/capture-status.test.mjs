import test from 'node:test';
import assert from 'node:assert/strict';
import { captureBackendNotice, capturePermissionCommand } from '../src/capture-status.js';

test('Packet Monitor permission failure does not recommend installing Npcap', () => {
  const result = captureBackendNotice('capture-error|Windows Packet Monitor|Cannot initialize (0x80070005); administrator privileges required');
  assert.equal(result.bad, true);
  assert.match(result.detail, /Packet Monitor.*80070005/);
  assert.doesNotMatch(result.title + result.detail, /Npcap/);
});
test('a running game without endpoints is a waiting state, not active capture', () => {
  const result = captureBackendNotice('waiting-for-game-connection');
  assert.equal(result.bad, false);
  assert.equal(result.compact, true);
  assert.match(result.title, /Waiting/);
});
test('Linux capture denial offers a capability command rather than Windows instructions', () => {
  const result = captureBackendNotice('no-access', value => value, { onLinux: true, binary: '/home/me/HS Tracker/hs-tracker' });
  assert.equal(result.bad, true);
  assert.doesNotMatch(result.detail, /Npcap/);
  assert.equal(result.fix, "sudo setcap cap_net_raw=ep '/home/me/HS Tracker/hs-tracker'");
  assert.equal(captureBackendNotice('no-access'), null, 'legacy Windows advice remains with its existing handler');
});
test('capability command preserves apostrophes and shell metacharacters inside the path', () => {
  assert.equal(capturePermissionCommand("/tmp/player's $(touch x)/hs-tracker"), "sudo setcap cap_net_raw=ep '/tmp/player'\\''s $(touch x)/hs-tracker'");
});
