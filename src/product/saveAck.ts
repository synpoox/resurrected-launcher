/** Save-breaking acknowledgements (issue 213): one per product and target release, kept in
 *  localStorage so a restart doesn't ask again for the same target; a different target release
 *  asks afresh. Not now never records one. If storage is unavailable the session set still
 *  remembers, and after a restart the launcher simply asks again. */
const acked = new Set<string>();

const key = (productId: string, releaseId: string) => `saveBreakingAck:${productId}\n${releaseId}`;

export const saveBreakingAcked = (productId: string, releaseId: string) => {
  const k = key(productId, releaseId);
  if (acked.has(k)) return true;
  try {
    return localStorage.getItem(k) === "1";
  } catch {
    return false;
  }
};

export const rememberSaveBreakingAck = (productId: string, releaseId: string) => {
  const k = key(productId, releaseId);
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
