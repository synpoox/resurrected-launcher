/** Save-breaking acknowledgements (issue 213): one per product and target release, kept for the
 *  session. A retry of the same update (after a failure or a cancel) doesn't ask again; a different
 *  target release asks afresh. Not now never records one. */
const acked = new Set<string>();

const key = (productId: string, releaseId: string) => `${productId}\n${releaseId}`;

export const saveBreakingAcked = (productId: string, releaseId: string) => acked.has(key(productId, releaseId));

export const rememberSaveBreakingAck = (productId: string, releaseId: string) => {
  acked.add(key(productId, releaseId));
};

/** For tests. */
export const resetSaveBreakingAcks = () => acked.clear();
