export const MAP_CHANNEL = 'hs-map';
export const MAP_VERSION = 1;
const finite = (value, min, max) => typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max;
const fields = (value, names) => value && typeof value === 'object' && !Array.isArray(value)
  && Object.keys(value).length === names.length && names.every(name => Object.hasOwn(value, name));

export function validMapState(state) {
  return !!(fields(state, ['camera', 'viewport', 'selection', 'presentation'])
    && fields(state.camera, ['center', 'scale']) && Array.isArray(state.camera.center)
    && state.camera.center.length === 2 && state.camera.center.every(value => finite(value, -4, 4))
    && finite(state.camera.scale, 0.000001, 1024)
    && fields(state.viewport, ['width', 'height']) && finite(state.viewport.width, 1, 32768)
    && finite(state.viewport.height, 1, 32768) && fields(state.selection, ['roomId'])
    && (state.selection.roomId === null || (typeof state.selection.roomId === 'string'
      && state.selection.roomId.length > 0 && state.selection.roomId.length <= 128
      && !/[\x00-\x1f\x7f]/.test(state.selection.roomId)))
    && fields(state.presentation, ['query', 'lang', 'source']) && typeof state.presentation.query === 'string'
    && state.presentation.query.length <= 256 && !/[\x00-\x1f\x7f]/.test(state.presentation.query)
    && ['en', 'de', 'sp', 'fi', 'fr', 'pl', 'pt', 'ru', 'zh', 'ja', 'ko'].includes(state.presentation.lang)
    && (state.presentation.source === null || (typeof state.presentation.source === 'string'
      && state.presentation.source.length > 0 && state.presentation.source.length <= 128
      && !/[\x00-\x1f\x7f]/.test(state.presentation.source)))
    && (state.presentation.source === null || state.selection.roomId === null));
}

export function mapEnvelope(message, type) {
  return message?.channel === MAP_CHANNEL && message.version === MAP_VERSION && message.type === type
    && Number.isSafeInteger(message.revision) && message.revision >= 0;
}

// The iframe draws at the SOURCE document size, then its whole image scales to
// fit. Different OBS dimensions add margins instead of revealing another view.
export function mirrorBox(source, target) {
  if (!source || !target || !finite(source.width, 1, 32768) || !finite(source.height, 1, 32768)
    || !finite(target.width, 1, 32768) || !finite(target.height, 1, 32768)) return null;
  const scale = Math.min(target.width / source.width, target.height / source.height);
  return { width: source.width, height: source.height, scale,
    left: (target.width - source.width * scale) / 2, top: (target.height - source.height * scale) / 2 };
}

export function sourceUrl(base, mode, parentOrigin) {
  const url = new URL('/hs-map/index.html', base);
  url.searchParams.set('parentOrigin', parentOrigin);
  url.searchParams.set('mode', mode);
  return url.href;
}
