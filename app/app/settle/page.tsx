import TxLink from "@/components/TxLink";
import { findTx, loadRun, usd } from "@/lib/run";

export const metadata = { title: "정산·배분 — SaveSail 데모" };
const PARTS = ["운영비", "선순위", "준비금 인출", "준비금 적립", "후순위"];
const COLORS = ["#9aa4ad", "#0e4f6e", "#e9b44c", "#c48a00", "#0f8b8d"];

export default function Settle() {
  const run = loadRun();
  const max = Math.max(...run.periods.map((p) => p.dist.reduce((a, b) => a + Number(b), 0)), 1);
  return (
    <>
      <h1>정산·배분: 운영비 → 선순위 → 준비금 → 후순위</h1>
      <p className="lead">지급자는 확정된 정산액만 낸다. 받은 돈은 운영비, 선순위 원리금, 준비금 순서로 채우고 나머지는 후순위로 간다.
        선순위가 모자라면 준비금에서 끌어온다. 투자자는 보유 단위만큼 언제든 청구한다.</p>
      <div className="scroll"><table>
        <thead><tr><th>분기</th><th className="n">정산액</th><th className="n">누적 E</th>{PARTS.map((x) => <th key={x} className="n">{x}</th>)}<th>배분 거래</th></tr></thead>
        <tbody>{run.periods.map((p) => (
          <tr key={p.period}><td>{p.period + 1}</td><td className="n">{usd(p.due, 2)}</td><td className="n">{usd(p.entitlement, 2)}</td>
            {p.dist.map((x, i) => <td key={i} className="n">{usd(x, 2)}</td>)}
            <td><TxLink run={run} sig={findTx(run, `distribute_${p.period}`)?.sig} /></td></tr>))}</tbody>
      </table></div>
      <h2>분기별 배분 막대</h2>
      <svg viewBox="0 0 640 220" width="100%" role="img" aria-label="분기별 배분 구성 막대 그래프">
        {run.periods.map((p, i) => {
          let x0 = 70;
          return (
            <g key={p.period}>
              <text x={60} y={30 + i * 48} fontSize="12" textAnchor="end">{p.period + 1}분기</text>
              {p.dist.map((v, k) => {
                const w = (Number(v) / max) * 540;
                const r = <rect key={k} x={x0} y={16 + i * 48} width={w} height={22} fill={COLORS[k]}><title>{`${PARTS[k]} ${usd(v, 2)} dUSD`}</title></rect>;
                x0 += w;
                return r;
              })}
            </g>
          );
        })}
      </svg>
      <div className="legend" aria-hidden="true">{PARTS.map((x, k) => <span key={x} style={{ ["--c" as string]: COLORS[k] }}>{x}</span>)}</div>
      <h2>청구</h2>
      <div className="scroll"><table>
        <thead><tr><th>투자자</th><th>트랜치</th><th>거래</th></tr></thead>
        <tbody>{[["개인 A", "선순위", "claim_investorA_0"], ["개인 B", "선순위", "claim_investorB_0"], ["용선주 C", "선순위", "claim_investorC_0"], ["용선주 C", "후순위", "claim_investorC_1"]].map(([w, t, s]) => (
          <tr key={s}><td>{w}</td><td>{t}</td><td><TxLink run={run} sig={findTx(run, s)?.sig} /></td></tr>))}</tbody>
      </table></div>
      <p className="muted">금고 잔액: 선순위 {usd(run.balances.senior_vault, 2)} · 후순위 {usd(run.balances.junior_vault, 2)} · 준비금 {usd(run.balances.reserve_vault, 2)} dUSD (청구 끝수 포함)</p>
    </>
  );
}
