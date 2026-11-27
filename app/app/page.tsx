import TxLink from "@/components/TxLink";
import { findTx, loadRun, OUTCOME, pct, usd } from "@/lib/run";

export default function Overview() {
  const run = loadRun();
  const last = run.periods[run.periods.length - 1];
  const d = run.deal;
  return (
    <>
      <h1>세일이 아낀 연료만큼만 받는 금융</h1>
      <p className="lead">
        풍력 보조 추진 장치(세일)는 연료를 줄이지만, 설치 전후 연비를 비교하면 날씨·선체 오염 같은 다른 요인이 섞여 효과를 믿기 어렵다.
        SaveSail은 세일을 켜고 끄는 일정을 세 참여자의 해시 체인으로 미리 체인에 고정하고, 무작위로 끈 구간과 비교해 절감률을 잰 뒤
        신뢰구간 하한만큼만 투자자에게 정산한다. 이 화면은 그 과정을 {run.cluster === "devnet" ? "devnet" : "로컬 체인(같은 프로그램 바이너리)"}에서 처음부터 끝까지 실행한 기록이다.
      </p>
      <div className="grid" role="list">
        <div className="card" role="listitem"><div className="k">설치비(선순위/후순위)</div><div className="v">{usd(d.install_cost)} dUSD</div><small>{usd(d.caps[0])} / {usd(d.caps[1])}</small></div>
        <div className="card" role="listitem"><div className="k">지급 비율 φ · 기준 절감률</div><div className="v">{d.phi_bps / 100}% · {d.ref_rate_bps / 100}%</div></div>
        <div className="card" role="listitem"><div className="k">1년차 캠페인</div><div className="v">{d.n_groups}그룹</div><small>3시간 블록 × 5, 그룹마다 OFF 1블록 · {(d.n_groups * 15).toLocaleString()} 항해시간 · {d.n_groups / d.groups_per_period}분기</small></div>
        <div className="card" role="listitem"><div className="k">누적 정산액 E</div><div className="v">{usd(run.deal.entitlement)} dUSD</div><small>마지막 분기 하한 {last ? pct(last.payload.lb_bps / 1e4) : "—"}</small></div>
        <div className="card" role="listitem"><div className="k">트랜잭션</div><div className="v">{run.tx_count.toLocaleString()}건</div><small>{run.cluster} · {run.seconds < 120 ? `${run.seconds}초` : `${Math.round(run.seconds / 60)}분`}</small></div>
      </div>
      <h2>분기별 결과</h2>
      <div className="scroll">
        <table>
          <caption className="skip">분기별 판정과 정산</caption>
          <thead><tr><th>분기</th><th>판정</th><th className="n">하한</th><th className="n">점추정</th><th className="n">상한</th><th className="n">정산액(dUSD)</th><th>정산 거래</th></tr></thead>
          <tbody>
            {run.periods.map((p) => (
              <tr key={p.period}>
                <td>{p.period + 1}분기</td><td>{OUTCOME[p.outcome]}</td>
                <td className="n">{pct(p.payload.lb_bps / 1e4)}</td><td className="n">{pct(p.payload.est_bps / 1e4)}</td><td className="n">{pct(p.payload.ub_bps / 1e4)}</td>
                <td className="n">{usd(p.due, 2)}</td><td><TxLink run={run} sig={findTx(run, `settle_${p.period}`)?.sig} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <h2>구조</h2>
      <ol>
        <li>개인(선순위)과 용선주(후순위)가 설치비를 모은다. 개인은 등급·연간 한도 검사를 거친다.</li>
        <li>지급자(선사)·투자자 대표·검증기관이 해시 체인 끝값을 등록한다. 그룹마다 검증기관이 먼저 값을 공개하고, 세 값을 합쳐 OFF 블록이 정해진다.</li>
        <li>운항 데이터와 세일 제어기 기록의 해시를 그룹마다 체인에 커밋한다. 데이터는 일정이 정해진 뒤에만 만들어진다.</li>
        <li>검증기관 3곳 중 2곳이 같은 결과에 서명하면 그 분기가 확정되고, 하한 기준 정산액이 운영비 → 선순위 → 준비금 → 후순위로 나뉜다.</li>
      </ol>
      <p className="muted">프로그램 <code>{run.program}</code> · 계약 <code>{run.accounts.deal}</code></p>
    </>
  );
}
