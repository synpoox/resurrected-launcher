import { useEffect, useState } from "react";
import { ping, type PingResponse } from "./ipc";

/** Empty shell: renders what Rust reports. The real layout arrives in slice 4. */
export function App() {
  const [info, setInfo] = useState<PingResponse | null>(null);

  useEffect(() => {
    ping().then(setInfo, () => setInfo(null));
  }, []);

  return <main>{info && <h1>{info.display_name}</h1>}</main>;
}
