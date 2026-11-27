import TxLink from "@/components/TxLink";
import { findTx, loadRun, usd } from "@/lib/run";

export const metadata = { title: "투자 — SaveSail 데모" };

export default function Invest() {
  const run = loadRun();
  const d = run.deal;
  const inv = [
    ["개인 A", "Retail", "선순위", "invest_A_senior"], ["개인 B", "Retail", "선순위", "invest_B_senior"],
    ["용선주 C", "Pro", "선순위", "invest_C_senior"], ["용선주 C", "Pro", "후순위", "invest_C_junior"],
  ];
  return (
    <>
      <h1>투자: 등급과 연간 한도</h1>
      <p className="lead">개인은 선순위만, 전문 투자자는 두 트랜치 모두 살 수 있다. 개인 한도는 연간 {usd(d.retail_limit)} dUSD(원화 1억 원 ÷ 1,430원, 데모 고정 환율)다.
        모집이 다 차면 설치비가 제조사로 이체된다.</p>
      <div className="grid">
        <div className="card"><div className="k">선순위 모집</div><div className="v">{usd(d.raised[0])} / {usd(d.caps[0])} dUSD</div><small>분기 원리금 {usd(d.senior_per_period, 2)} dUSD × {d.n_periods}분기</small></div>
        <div className="card"><div className="k">후순위 모집</div><div className="v">{usd(d.raised[1])} / {usd(d.caps[1])} dUSD</div><small>남는 현금흐름 전부</small></div>
      </div>
      <h2>투자 기록</h2>
      <div className="scroll"><table>
        <thead><tr><th>투자자</th><th>등급</th><th>트랜치</th><th>거래</th></tr></thead>
        <tbody>{inv.map(([who, tier, tr, step]) => (
          <tr key={step}><td>{who}</td><td>{tier}</td><td>{tr}</td><td><TxLink run={run} sig={findTx(run, step)?.sig} /></td></tr>))}
          <tr><td>모집 마감</td><td colSpan={2}>설치비 {usd(d.install_cost)} dUSD → 제조사</td><td><TxLink run={run} sig={findTx(run, "close_fundraising")?.sig} /></td></tr>
        </tbody>
      </table></div>
      <h2>프로그램이 거부한 시도</h2>
      <div className="scroll"><table>
        <thead><tr><th>시도</th><th>오류 코드</th></tr></thead>
        <tbody>{run.steps.map((s) => (
          <tr key={s.step}><td>{s.step === "invest_A_junior" ? "개인 A가 후순위 매수" : s.step === "invest_A_over_limit" ? "개인 A가 한도 초과 매수" : s.step}</td>
            <td><span className="tag bad">{s.rejected}</span></td></tr>))}</tbody>
      </table></div>
      <h2>최종 잔액</h2>
      <div className="scroll"><table>
        <thead><tr><th>계정</th><th className="n">dUSD</th></tr></thead>
        <tbody>{["investorA", "investorB", "investorC", "manufacturer"].map((k) => (
          <tr key={k}><td>{({ investorA: "개인 A", investorB: "개인 B", investorC: "용선주 C", manufacturer: "제조사" } as Record<string, string>)[k]}</td>
            <td className="n">{usd(run.balances[k], 2)}</td></tr>))}</tbody>
      </table></div>
    </>
  );
}
