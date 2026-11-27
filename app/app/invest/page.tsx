import TxLink from "@/components/TxLink";
import { explorerOf, findTx, loadP1, loadRun, usd } from "@/lib/run";

function Ext({ cluster, kind, id }: { cluster: string; kind: "tx" | "address"; id: string | null }) {
  if (!id) return <span className="muted">—</span>;
  const href = explorerOf(cluster, kind, id);
  const text = `${id.slice(0, 6)}…${id.slice(-4)}`;
  return href ? <a href={href} rel="noopener" target="_blank">{text}<span className="skip"> (탐색기에서 열기)</span></a> : <code title={id}>{text}</code>;
}
const P1_NAMES: Record<string, string> = { investorA: "개인 A", investorB: "개인 B", outsider: "KYC 통과, 목록 밖 지갑" };
const P1_OK: [string, string][] = [["thaw_A_self", "개인 A가 스스로 동결 해제"], ["invest_tokenized_A", "개인 A 투자 → 선순위 토큰 발행"],
  ["thaw_B_self", "개인 B가 스스로 동결 해제"], ["transfer_A_to_B", "A → B 이전(훅이 양쪽 배분 정산)"]];
const P1_FAIL: Record<string, string> = { thaw_outsider_self: "목록 밖 지갑이 스스로 동결 해제", thaw_outsider_by_A: "개인 A가 목록 밖 지갑을 대신 해제",
  invest_tokenized_outsider: "목록 밖 지갑 투자(토큰 수령)", transfer_A_to_outsider: "A → 목록 밖 지갑 이전" };

export const metadata = { title: "투자 — SaveSail 데모" };

export default function Invest() {
  const run = loadRun();
  const p1 = loadP1();
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
      {p1 && (<>
        <h2>수익권 토큰: 허용 목록 지갑만 받는다</h2>
        <p className="lead">선순위·후순위는 각각 Token-2022 토큰이다. 새 토큰 계정은 동결 상태로 만들어지고, Token ACL 허용 목록에 오른 지갑만
          스스로 동결을 풀 수 있다. 이전할 때마다 Transfer Hook이 보내는 쪽·받는 쪽의 배분을 먼저 정산한다. ({p1.cluster}, 별도 데모 계약)</p>
        <div className="scroll"><table>
          <thead><tr><th>트랜치</th><th>토큰 민트</th><th>새 계정 기본 상태</th><th>허용 목록 게이트</th><th className="n">발행량</th></tr></thead>
          <tbody>{p1.mints.map((m) => (
            <tr key={m.address}><td>{m.tranche === "senior" ? "선순위" : "후순위"}</td><td><Ext cluster={p1.cluster} kind="address" id={m.address} /></td>
              <td>{m.default_account_state === "Frozen" ? "동결" : m.default_account_state}</td><td><Ext cluster={p1.cluster} kind="address" id={m.token_acl} /></td>
              <td className="n">{usd(m.supply)}</td></tr>))}</tbody>
        </table></div>
        <div className="scroll"><table>
          <thead><tr><th>지갑</th><th>허용 목록</th><th>토큰 계정</th><th>지금</th><th className="n">토큰 잔액</th><th className="n">포지션 단위</th></tr></thead>
          <tbody>{p1.holders.map((h) => (
            <tr key={h.name}><td>{P1_NAMES[h.name] ?? h.name}</td><td>{h.listed ? <span className="tag ok">있음</span> : <span className="tag bad">없음</span>}</td>
              <td><Ext cluster={p1.cluster} kind="address" id={h.account} /></td><td>{h.state === "frozen" ? "동결" : "해제"}</td>
              <td className="n">{usd(h.balance)}</td><td className="n">{usd(h.units)}</td></tr>))}</tbody>
        </table></div>
        <div className="scroll"><table>
          <thead><tr><th>시도</th><th>결과</th><th>거래</th></tr></thead>
          <tbody>
            {P1_OK.map(([k, label]) => (
              <tr key={k}><td>{label}</td><td><span className="tag ok">성공</span></td>
                <td><Ext cluster={p1.cluster} kind="tx" id={p1.steps.find((s) => s.step === k)?.signature ?? null} /></td></tr>))}
            {p1.rejected.map((f) => (
              <tr key={f.label}><td>{P1_FAIL[f.label] ?? f.label}</td>
                <td><span className="tag bad">{f.expect.includes("frozen") ? "거부: 계정 동결" : "거부: 허용 목록 밖"}</span></td>
                <td><Ext cluster={p1.cluster} kind="tx" id={f.signature} /></td></tr>))}
          </tbody>
        </table></div>
      </>)}
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
