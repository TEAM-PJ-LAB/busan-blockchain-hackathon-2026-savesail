"use client";
// PRD 7.5 acceptance 1-2: compares the numbers on the page (run.json) with the live devnet accounts.
import { useEffect, useState } from "react";
import { compareChain, compareP1, rpcFetch, type ChainView, type P1View, type Result } from "@/lib/chaincheck";

const RPC = "https://api.devnet.solana.com";

export default function ChainCheck({ run, p1 }: { run: ChainView; p1?: P1View | null }) {
  const [state, setState] = useState<Result | "loading" | "offline" | "local">(run.cluster === "devnet" ? "loading" : "local");
  useEffect(() => {
    if (run.cluster !== "devnet") return;
    const f = rpcFetch(RPC);
    // P1 token values (invest page) are compared too when their record is from devnet
    Promise.all([compareChain(run, f), p1?.cluster === "devnet" ? compareP1(p1, f) : Promise.resolve({ total: 0, bad: [] })])
      .then(([a, b]) => setState({ total: a.total + b.total, bad: [...a.bad, ...b.bad] }), () => setState("offline"));
  }, [run, p1]);
  if (state === "local") return <p className="chk">로컬 실행 기록이다. devnet 실행 기록으로 배포하면 이 자리에서 체인 계정과 자동 대조한다.</p>;
  if (state === "loading") return <p className="chk" aria-live="polite">devnet 계정과 대조하는 중…</p>;
  if (state === "offline") return <p className="chk" aria-live="polite"><span className="tag warn">RPC 응답 없음</span> devnet이 응답하지 않아 실행 기록(run.json)의 값으로 표시한다.</p>;
  const ok = state.total - state.bad.length;
  return (
    <p className="chk" aria-live="polite">
      <span className={`tag ${state.bad.length ? "bad" : "ok"}`}>체인 대조 {state.bad.length ? "불일치" : "일치"}</span>{" "}
      화면 값 {state.total}개 중 {ok}개가 devnet 계정과 같다{state.bad.length ? `: ${state.bad.slice(0, 8).join(", ")}` : ""}.
    </p>
  );
}
