import TxLink from "@/components/TxLink";
import { findTx, loadRun, OUTCOME, pct } from "@/lib/run";

export const metadata = { title: "검증 — SaveSail 데모" };
const FLAG = ["공변량 불균형", "유량계 검사", "누적 데이터 부족", "데이터 품질"];

function Chart({ rows }: { rows: { p: number; lb: number; est: number; ub: number }[] }) {
  const W = 640, H = 260, pad = 44;
  const lo = Math.min(...rows.map((r) => r.lb)) - 0.005, hi = Math.max(...rows.map((r) => r.ub)) + 0.005;
  const x = (i: number) => pad + (i + 0.5) * ((W - pad * 1.5) / rows.length);
  const y = (v: number) => H - pad - ((v - lo) / (hi - lo)) * (H - pad * 1.6);
  const ticks = Array.from({ length: 5 }, (_, i) => lo + ((hi - lo) * i) / 4);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} width="100%" role="img" aria-label="분기별 누적 절감률 신뢰구간: 하한, 점추정, 상한">
      {ticks.map((t) => (<g key={t}><line x1={pad} x2={W - 8} y1={y(t)} y2={y(t)} stroke="#d8dde3" /><text x={pad - 6} y={y(t) + 4} fontSize="11" textAnchor="end" fill="#55636f">{(t * 100).toFixed(1)}%</text></g>))}
      {rows.map((r, i) => (
        <g key={r.p}>
          <line x1={x(i)} x2={x(i)} y1={y(r.lb)} y2={y(r.ub)} stroke="#0e4f6e" strokeWidth="3" />
          <line x1={x(i) - 10} x2={x(i) + 10} y1={y(r.lb)} y2={y(r.lb)} stroke="#0e4f6e" strokeWidth="3" />
          <line x1={x(i) - 10} x2={x(i) + 10} y1={y(r.ub)} y2={y(r.ub)} stroke="#0e4f6e" strokeWidth="2" />
          <circle cx={x(i)} cy={y(r.est)} r="5" fill="#0f8b8d" />
          <text x={x(i)} y={H - pad + 18} fontSize="12" textAnchor="middle" fill="#0b1f33">{r.p + 1}분기</text>
        </g>
      ))}
    </svg>
  );
}

export default function Verify() {
  const run = loadRun();
  const d = run.deal;
  const rows = run.periods.map((p) => ({ p: p.period, lb: p.payload.lb_bps / 1e4, est: p.payload.est_bps / 1e4, ub: p.payload.ub_bps / 1e4 }));
  return (
    <>
      <h1>검증: 무작위 OFF 구간과 비교한 누적 절감률</h1>
      <p className="lead">절감률 = 1 − (ON 실제/ON 예측) ÷ (OFF 실제/OFF 예측). 예측은 설치 전 1년 데이터로 맞춘 기준 모델이고, 신뢰구간은 설치 전 데이터에 같은 무작위 일정을 가상으로 적용해 얻은 반사실 오차 분포로 만든다(교차 적합 위약 보정).
        분기마다 지금까지 쌓인 데이터 전체로 다시 계산한다.</p>
      <Chart rows={rows} />
      <div className="scroll"><table>
        <thead><tr><th>분기</th><th className="n">하한</th><th className="n">점추정</th><th className="n">상한</th><th className="n">준수율</th><th>검사</th><th>판정</th><th>승인 2/3</th><th>보고서 해시</th></tr></thead>
        <tbody>{run.periods.map((p) => (
          <tr key={p.period}>
            <td>{p.period + 1}</td><td className="n">{pct(p.payload.lb_bps / 1e4)}</td><td className="n">{pct(p.payload.est_bps / 1e4)}</td>
            <td className="n">{pct(p.payload.ub_bps / 1e4)}</td><td className="n">{(p.payload.compliance_bps / 100).toFixed(1)}%</td>
            <td>{p.payload.flags ? FLAG.filter((_, i) => p.payload.flags >> i & 1).join(", ") : <span className="tag ok">이상 없음</span>}</td>
            <td>{OUTCOME[p.outcome]}</td>
            <td><TxLink run={run} sig={findTx(run, `approve_${p.period}_verifier0`)?.sig} label="1" /> · <TxLink run={run} sig={findTx(run, `approve_${p.period}_verifier1`)?.sig} label="2" /></td>
            <td><code>{p.payload.report_hash.slice(0, 16)}…</code></td>
          </tr>))}</tbody>
      </table></div>
      <h2>판정 규칙</h2>
      <ul>
        <li>누적 1,800 항해시간 전에는 하한을 쓰지 않는다(이 데모의 1분기는 {(d.groups_per_period * 15).toLocaleString()}시간이라 지급 0). 다음 분기에 앞선 연료까지 다시 평가해 따라잡는다.</li>
        <li>OFF 블록에서 세일을 켜면 준수율이 떨어지고, {d.comp_min_bps / 100}% 미만이면 그 분기는 상한과 기준 절감률({d.ref_rate_bps / 100}%) 중 큰 값으로 지급자에게 불리하게 정산한다.</li>
        <li>투자자 대표가 허용 상한({d.miss_max_bps / 100}%)을 넘겨 공개를 거부하면 그 분기 정상 연료는 0으로 정산한다.</li>
      </ul>
    </>
  );
}
