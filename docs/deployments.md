# devnet 배포 기록

| 시각(UTC) | 커밋 | 배포 슬롯 | 바이너리 sha256 앞 16자 | 비고 |
|---|---|---|---|---|


## G1 devnet 전체 흐름 3회 (2026-11-27, 워크플로 실행 45957003982)

프로그램 [`94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn`](https://explorer.solana.com/address/94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn?cluster=devnet). 매회 새로 배포(같은 주소에 바이너리 다시 올림) 후 투자 → 일정 고정 → 240그룹 공개 → 데이터 커밋 → 4개 기간 검증·정산·배분 → 청구를 끝까지 보냈다. 기록은 브랜치 `devnet-runs-45957003982`, 웹(GitHub Pages)은 3회차 기록을 보여 준다.

| 회차 | 계약(Deal) | 트랜잭션 | 걸린 시간(초, 기준 ≤ 1,800) | 서명 확인 | 화면 값 체인 대조 | 분기별 지급액(dUSD) |
|---|---|---|---|---|---|---|
| 1 | [`47VaEDqw…`](https://explorer.solana.com/address/47VaEDqwyhkd6eYX8wtHLX7AGCc8fFS5FbbJ5gjBiPT4?cluster=devnet) | 525 | 1,489 | 525/525 | 333/333 | 0 / 93,857 / 87,014 / 0 |
| 2 | [`FiHcvDzU…`](https://explorer.solana.com/address/FiHcvDzUyDSVCFNofcFKNHbeeg3iBFovQbWjPiMz8aiR?cluster=devnet) | 525 | 1,483 | 525/525 | 333/333 | 0 / 90,329 / 83,862 / 0 |
| 3 | [`9jKS89tc…`](https://explorer.solana.com/address/9jKS89tcLjN2yyjXwpnw4MxGVN3bXLAhAkkkRb32VGHQ?cluster=devnet) | 525 | 1,472 | 525/525 | 333/333 | 0 / 88,943 / 83,672 / 0 |

회차마다 계약 주소가 달라 무작위 일정과 합성 데이터가 다르므로 분기별 지급액도 다르다. 결과 유형(1분기 그룹 부족 0, 2분기 정상, 3분기 OFF 위반 불리 정산, 4분기 투자자 대표 거부 상한 초과)은 세 회차 모두 같다.

**3회차 주요 트랜잭션(탐색기)**

| 단계 | 트랜잭션 |
|---|---|
| 계약 생성 | [`3vNrpUHEY6ruK43o…`](https://explorer.solana.com/tx/3vNrpUHEY6ruK43oqtStRYUYxup5YLzcExGz4N6ktCcGUFgrLe87uoUU7TtXXMB68WKUvERyFUKbcdMizbeuuDkR?cluster=devnet) |
| 선순위 투자 | [`4gduf41QRt2x3dyr…`](https://explorer.solana.com/tx/4gduf41QRt2x3dyrukMv7EXcRpBxhuTi2WuHRhRvUpx95D1U8tZhJT2gnJrssEMcsZ3wLHrkGPYyLoDAj4WNe7Yp?cluster=devnet) |
| 0번 그룹 공개(검증기관 → 지급자 → 투자자 대표) | [`XoRv53YHem6VpWpj…`](https://explorer.solana.com/tx/XoRv53YHem6VpWpjneqCVtxvuBDpApAcfzm4u4hUjbYhTGfHebV2h293ezT65XQGR2Vk3WokPLdN6sbnJuQ1mKP?cluster=devnet) |
| 0번 그룹 데이터 커밋(운항·제어기) | [`5f8TL67YjTYn3AVs…`](https://explorer.solana.com/tx/5f8TL67YjTYn3AVsjvKizkrywpkrFNXWXJrdJvV38xoX7CLttJDmzt2ksiFgzHShLvisHKNq7NUGsZa7gyfRteK3?cluster=devnet) |
| 2분기 결과 두 번째 승인(확정) | [`3mopKQg1qaA2i3ye…`](https://explorer.solana.com/tx/3mopKQg1qaA2i3yemejFBX7DLkDHidUHHhJxWApkcYemXqhurNEsCUX9iDVvtmhNpJgnmFYEptP5uerMmDeu3MVx?cluster=devnet) |
| 2분기 정산 | [`q63F3qdWsAQwFXDg…`](https://explorer.solana.com/tx/q63F3qdWsAQwFXDgPufoanvZV1mEE3kH4225unzy6gtjCQsBcMaYjdXWoFxaWwej6DCHRFuyQQEBazhow3zpu5k?cluster=devnet) |
| 2분기 배분 | [`4wNTH49Lh41x35yb…`](https://explorer.solana.com/tx/4wNTH49Lh41x35ybrmdRMXTjowranGd7rGwAXVPT6LA8KdLeDJsNCXu98fg7VLFSGDANpPbw7aVWrCQymnar6tYS?cluster=devnet) |
| 투자자 A 청구 | [`4eGFQFX16mUC2DRd…`](https://explorer.solana.com/tx/4eGFQFX16mUC2DRdMHsSs2dUWCTR2wEXkdgg7HPxbYDRKQNBJHkgeGL13fLdqXTZA6Eg62rCzwR16TVQmQsVHrpz?cluster=devnet) |
| 보증금 반환 | [`2kyVaShxhojddCVs…`](https://explorer.solana.com/tx/2kyVaShxhojddCVsmYA1guMpCd9yxcqeB2fMtAN69d5umWYzVFNP1yCsF1jTnsyJmbhZcApjYHzpK2ABkKsmbpKE?cluster=devnet) |
| 2026-11-27T09:52:44Z | 08b1112 | 526829019 | ffba5ccc26aedb80 | P1 |
