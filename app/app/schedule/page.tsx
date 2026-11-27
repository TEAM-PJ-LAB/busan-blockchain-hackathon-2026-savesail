import TxLink from "@/components/TxLink";
import { findTx, loadRun } from "@/lib/run";

export const metadata = { title: "일정 — SaveSail 데모" };
const NO_OFF = 7;

export default function Schedule() {
  const run = loadRun();
  const VIOL = new Set(run.scenario?.off_violations ?? []);
  const gpp = run.scenario?.groups_per_period ?? 60;
  const offs = [0, 0, 0, 0, 0];
  run.schedule.forEach((s) => s.off !== NO_OFF && offs[s.off]++);
  const miss = run.schedule.map((s, g) => [g, s.missing] as const).filter(([, m]) => m);
  const role = (m: number) => ["지급자", "투자자 대표", "검증기관"].filter((_, i) => m >> i & 1).join("·");
  return (
    <>
      <h1>일정: 아무도 고를 수 없는 OFF 블록</h1>
      <p className="lead">그룹마다 검증기관이 먼저 해시 체인 값을 공개하고, 지급자와 투자자 대표가 뒤따른다. 세 값을 합친 sha256으로 OFF 블록이 정해진다.
        공개를 거부한 그룹은 모든 블록을 켠 채로 두고 추정에서 빼며, 거부한 쪽의 보증금을 깎는다.</p>
      <div className="legend" aria-hidden="true">
        <span style={{ ["--c" as string]: "var(--on)" }}>ON</span><span style={{ ["--c" as string]: "var(--off)" }}>OFF</span>
        <span style={{ ["--c" as string]: "var(--bad)" }}>OFF 블록 위반</span><span style={{ ["--c" as string]: "var(--miss)" }}>공개 거부(전 블록 ON)</span>
      </div>
      <div className="sched" role="img" aria-label={`${run.schedule.length}그룹 × 5블록 일정. OFF 위치 분포 ${offs.join(", ")}. 공개 거부 ${miss.length}그룹.`}>
        {run.schedule.map((s, g) => (
          <div key={g} className={`g${s.missing ? " miss" : ""}${VIOL.has(g) ? " viol" : ""}`} title={`${g}번 그룹`}>
            {[0, 1, 2, 3, 4].map((b) => <i key={b} className={s.off === b ? "off" : undefined} />)}
          </div>
        ))}
      </div>
      <p><small>블록 위치별 OFF 횟수(0~4): {offs.join(" · ")}</small></p>
      <h2>공개 거부</h2>
      <div className="scroll"><table>
        <thead><tr><th>그룹</th><th>거부한 쪽</th><th>처리 거래</th></tr></thead>
        <tbody>{miss.map(([g, m]) => (
          <tr key={g}><td>{g}번({Math.floor(g / gpp) + 1}분기)</td><td>{role(m)}</td><td><TxLink run={run} sig={findTx(run, `resolve_missing_${g}`)?.sig} /></td></tr>))}</tbody>
      </table></div>
      <h2>앵커와 공개 거래</h2>
      <ul>
        {[0, 1, 2].map((r) => <li key={r}>{["지급자", "투자자 대표", "검증기관"][r]} 앵커 등록: <TxLink run={run} sig={findTx(run, `register_anchor_${r}`)?.sig} /></li>)}
        <li>0번 그룹 공개(검증기관 → 지급자 → 투자자 대표): <TxLink run={run} sig={findTx(run, "reveal_0")?.sig} /></li>
      </ul>
    </>
  );
}
