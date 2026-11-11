/** Save-breaking acknowledgements (issue 213, 215): one per product, installed release and target, kept in
 *  localStorage so a restart doesn't ask again for the same move; a different target or installed release
 *  asks afresh. Not now never records one. If storage is unavailable the session set still
 *  remembers, and after a restart the launcher simply asks again. */
const acked = new Set<string>();

const key = (productId: string, installedId: string | undefined, releaseId: string) =>
  `saveBreakingAck:${productId}\n${installedId ?? ""}\n${releaseId}`;

export const saveBreakingAcked = (productId: string, installedId: string | undefined, releaseId: string) => {
  const k = key(productId, installedId, releaseId);
  if (acked.has(k)) return true;
  try {
    return localStorage.getItem(k) === "1";
  } catch {
    return false;
  }
};

export const rememberSaveBreakingAck = (productId: string, installedId: string | undefined, releaseId: string) => {
  const k = key(productId, installedId, releaseId);
  acked.add(k);
  try {
    localStorage.setItem(k, "1");
  } catch {
    // Storage unavailable: asked again after a restart.
  }
};

/** For tests. */
export const resetSaveBreakingAcks = () => {
  acked.clear();
  try {
    for (const k of Object.keys(localStorage)) if (k.startsWith("saveBreakingAck:")) localStorage.removeItem(k);
  } catch {
    // ignore
  }
};
