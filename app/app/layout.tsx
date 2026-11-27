import type { Metadata } from "next";
import "./globals.css";
import Nav from "./nav";
import ChainCheck from "@/components/ChainCheck";
import { chainView } from "@/lib/chaincheck";
import { loadRun } from "@/lib/run";

export const metadata: Metadata = {
  title: "SaveSail 데모 — 무작위 ON/OFF로 검증하는 윈드세일 성과 연동 금융",
  description: "세일 절감을 체인에 미리 고정한 무작위 일정으로 측정하고, 측정 하한만큼만 정산하는 Busan RWA 데모(devnet).",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="ko">
      <body>
        <a className="skip" href="#main">본문으로 건너뛰기</a>
        <header><div className="in"><a className="brand" href={`${process.env.NEXT_PUBLIC_BASE_PATH ?? ""}/`}>SaveSail</a><Nav /></div></header>
        <main id="main"><ChainCheck run={chainView(loadRun())} />{children}</main>
        <footer>모든 수치는 실행 기록(run.json)과 체인 계정에서 읽는다. 테스트 토큰(dUSD)만 쓰는 devnet 데모이며 투자 권유가 아니다.</footer>
      </body>
    </html>
  );
}
