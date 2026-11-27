# devnet 배포 기록

| 시각(UTC) | 커밋 | 배포 슬롯 | 바이너리 sha256 앞 16자 | 비고 |
|---|---|---|---|---|
| 2026-11-27T09:52:44Z | 08b1112 | 526829019 | ffba5ccc26aedb80 | P1 |
| 2026-11-27T10:07:14Z | a220493 | 526832631 | ffba5ccc26aedb80 | P1 |


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

## P1 devnet 실행 (2026-11-27, 워크플로 실행 45979031286)

프로그램 [`94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn`](https://explorer.solana.com/address/94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn?cluster=devnet)을 P1 바이너리(sha256 앞 16자 `ffba5ccc26aedb80`)로 다시 올린 뒤 `scripts/run/p1.ts devnet`을 돌렸다: 24건 성공, 실패해야 하는 4건은 사전 시뮬레이션 없이 보내 체인에 실패로 남김, 82초, 점검 11/11. 웹 투자 화면이 보여 주는 P1 값 21개를 devnet 계정과 대조해 21/21 일치(값을 하나씩 바꾼 6종은 모두 불일치로 잡힘). 같은 실행에서 devnet의 Token ACL·ABL 게이트 바이너리를 내려받아 P1 LiteSVM 테스트 13건을 그 바이너리로도 돌려 13/13(저장소 고정 바이너리로도 13/13). 기록은 브랜치 `devnet-p1-45979031286`, 요약은 `app/public/p1-run.json`. 첫 실행(45976185555, 계약 `AsR48hi5…`)도 같은 결과(11/11)였다.

| 항목 | 주소 | 확인한 값 |
|---|---|---|
| Token ACL / ABL 게이트(devnet 배포본) | [`TACLkU6C…`](https://explorer.solana.com/address/TACLkU6CiCdkQN2MjoyDkVg2yAH9zkxiHDsiztQ52TP?cluster=devnet) / [`GATEzzqx…`](https://explorer.solana.com/address/GATEzzqxhJnsWF6vHRsgtixxSB8PaQdcqGEVTEHWiULz?cluster=devnet) | 실행 가능 계정, 내려받은 바이너리 sha256 앞 16자 `bd467aa222517ac7` / `88a03cc277037bc0` |
| 계약(Deal, 모집 단계) | [`GbuDY7uj…`](https://explorer.solana.com/address/GbuDY7ujEvonzEs9tBup4xsZJTGNKsvxnXf2K4CiPG1H?cluster=devnet) | `tranche_mints` = 아래 두 민트 |
| 선순위 민트 | [`GFnXKJxR…`](https://explorer.solana.com/address/GFnXKJxRKiLgqhApzy3ZHesuHDQXBQGaHXSxRyVUAN6Q?cluster=devnet) | 확장 MetadataPointer, TransferHook, DefaultAccountState, TokenMetadata. 새 계정 기본 상태 **Frozen**, 메타데이터 `token_acl` = 게이트, 훅 = SaveSail(변경 권한 없음), 발행 권한 = Deal PDA, 공급 10,000 |
| 선순위 Token ACL MintConfig | [`8HA78Y6r…`](https://explorer.solana.com/address/8HA78Y6rtSsf5xhPE1a7T9k8cqTEkvrWHAAKmVnEuSKB?cluster=devnet) | 게이트 프로그램 = ABL 게이트, 무허가 동결 해제 켜짐, 민트의 동결 권한 = 이 계정 |
| 후순위 민트 | [`5e1F8TTn…`](https://explorer.solana.com/address/5e1F8TTn1EWnbs44ta677KGnFapHveFZc6BAY836iWfp?cluster=devnet) | 확장 MetadataPointer, TransferHook, DefaultAccountState, TokenMetadata. 새 계정 기본 상태 **Frozen**, 메타데이터 `token_acl` = 게이트, 훅 = SaveSail(변경 권한 없음), 발행 권한 = Deal PDA, 공급 0 |
| 후순위 Token ACL MintConfig | [`JCkeyyBF…`](https://explorer.solana.com/address/JCkeyyBFj27nabTpahDXsipWvNLGAV38QDtFXkUSy4UH?cluster=devnet) | 게이트 프로그램 = ABL 게이트, 무허가 동결 해제 켜짐, 민트의 동결 권한 = 이 계정 |
| ABL 허용 목록 | [`Bp6JtFQH…`](https://explorer.solana.com/address/Bp6JtFQHdApeJGh6FHdgjdeMcei8cWRx7Fkm7eKFnajW?cluster=devnet) | 모드 allow, 지갑 2개(투자자 A [`6SfEYwWi…`](https://explorer.solana.com/address/6SfEYwWimadQ9AjwrEgkzrePKEdLrXTJoNVGQRFH2h7M?cluster=devnet), B [`4QhiHtyj…`](https://explorer.solana.com/address/4QhiHtyjCmjJFMTMsz82cvyveKWY3j6w8AQCwcb8YAJz?cluster=devnet)) |

| 보유자 | 목록 | 선순위 토큰 계정 | 만들 때 | 지금 | 잔액 = 포지션 단위 |
|---|---|---|---|---|---|
| 투자자 A | 있음 | [`HU78Er4N…`](https://explorer.solana.com/address/HU78Er4NaZ5d4rEWRDfKVaKEQXf7Z3p8GFAnCtahxvyw?cluster=devnet) | 동결 | 해제 | 7,500 = 7,500 |
| 투자자 B | 있음 | [`9aeeGMnm…`](https://explorer.solana.com/address/9aeeGMnm4wArFSdo7yQCBBw6hbhie636WeK1ZXV4DeL5?cluster=devnet) | 동결 | 해제 | 2,500 = 2,500 |
| KYC 통과·목록 밖 지갑 | 없음 | [`F9ohZmHi…`](https://explorer.solana.com/address/F9ohZmHiTP8SdCYn1eiA5VNaDuM62BMrQfZooEKddT1t?cluster=devnet) | 동결 | 동결 | 0 = 0 |

| 단계 | 결과 | 트랜잭션 |
|---|---|---|
| ABL 목록 생성 | 성공 | [`RfBNRyEsHfKWsZzA…`](https://explorer.solana.com/tx/RfBNRyEsHfKWsZzAJ8XLeYxRnN5MbcSG7jVvaHXSQfgxrcUskVjn4vDE6zkLTkKYjokaz9wj3YGWRb8YdyRG6E6?cluster=devnet) |
| 선순위 민트 생성(확장·메타데이터·발행 권한 이전) | 성공 | [`4q9z65hhwaYtYavD…`](https://explorer.solana.com/tx/4q9z65hhwaYtYavDCXhjZPiRutDTFufNVdoXthWkrYQbk6b8gSYxEgWbETHfxTCPtvXTHZgenUo6WJCApaTzRypt?cluster=devnet) |
| Token ACL MintConfig·무허가 해제·게이트 메타 | 성공 | [`5vecTxuQEptRU2k1…`](https://explorer.solana.com/tx/5vecTxuQEptRU2k1yCMxtgKNLx4NoqtC2rrF3G3advPeTXPFAJcJBbQuy9xxktYaNXxwL6E7EarJAuMzaKtMTZnf?cluster=devnet) |
| 트랜치 민트 등록(규칙 검사) | 성공 | [`4Q99csFQAgNFbvX5…`](https://explorer.solana.com/tx/4Q99csFQAgNFbvX58kaB2981eTZwT4NV5nqCRERQDwpBpwnVW4xLXMg264J41pCpx88zctPXpVke16YqBoodSwgj?cluster=devnet) |
| 투자자 A 스스로 동결 해제 | 성공 | [`GieGMD77BPQJzXU8…`](https://explorer.solana.com/tx/GieGMD77BPQJzXU83qmXpF4BPoeNwnNCLveD6pyvYXmXfU66iyQqgfAzdgTwW98WrR7rjhz1iQdH56jAyydZYXZ?cluster=devnet) |
| 투자자 A 투자(10,000 dUSD → 선순위 토큰 10,000) | 성공 | [`56WjQSKTeNDbJAAh…`](https://explorer.solana.com/tx/56WjQSKTeNDbJAAhvwGHeZ1oNxLoxpxHJkvVUq5F5SrK8vJxMw8sv7eizi1g9xpcRjkjkceCwnVyFd1pTxZHAC6s?cluster=devnet) |
| 투자자 B 스스로 동결 해제 | 성공 | [`2tmscMUqmF491Psx…`](https://explorer.solana.com/tx/2tmscMUqmF491Psxaz98wxtYHRUtLsuWta6qVHX5gN3gtwDf5FaFPHSHLp7euYesbriFz4v74r1taZwwZ7MMiQcr?cluster=devnet) |
| A → B 이전 2,500(Transfer Hook 정산) | 성공 | [`1NVjyyFaXfBGGFiA…`](https://explorer.solana.com/tx/1NVjyyFaXfBGGFiAiYXyZmjjm52c5BTRHfRdspjaWcwp2u4yVjm2vPxX8TDALTRYLndFnogE3qwqMo864c6atx?cluster=devnet) |
| 목록 밖 지갑 스스로 동결 해제 | 실패: 게이트 `AccountBlocked`(4098) | [`5VsdSVRGaceTh9AD…`](https://explorer.solana.com/tx/5VsdSVRGaceTh9ADBjx5ZzpDx7XLcVHq4QNT866bPLsP8nc85iPx8WMhrj5d3ooCMfv96nTddqYkmhwxrFqJyJNC?cluster=devnet) |
| 목록 지갑(A)이 목록 밖 지갑 해제 시도 | 실패: 게이트 `AccountBlocked`(4098) | [`2mf8DxtqdgviZvGP…`](https://explorer.solana.com/tx/2mf8DxtqdgviZvGPYC6qP3cbrz9yvhie27Tktnx3qTxU2cSKJFF475ofwFe97BKNe7jLK1bQ2e5FVgS3KicPNioM?cluster=devnet) |
| 목록 밖 지갑 투자(발행 수령) | 실패: Token-2022 `Account is frozen` | [`2fj3omA17qroXNRk…`](https://explorer.solana.com/tx/2fj3omA17qroXNRk2asetkTmMHnqQ6bwyDXDsQhs5LDWyABLzcY4nFzV5MjBCqwVZTGCCS7f4vhXfRUV79MMruru?cluster=devnet) |
| A → 목록 밖 지갑 이전(수령) | 실패: Token-2022 `Account is frozen` | [`3YnPqCzB6ii2qn6X…`](https://explorer.solana.com/tx/3YnPqCzB6ii2qn6XkUYDV6uvE2c81H7JmwNMoXv5bQwLeGoajELcy8xEQ19iDhaWCecsGraHBv4GHCVSh4Kc9fQ5?cluster=devnet) |

점검 11/11 통과: 트랜치마다 민트 1개, 기본 동결, `token_acl` 메타데이터, 훅 = SaveSail·변경 불가, MintConfig 게이트·무허가 해제, 허용 목록, 새 계정 동결 생성, 목록 지갑 해제·수령, 목록 밖 지갑 해제·수령 실패, 단위 = 잔액, 공급 = 모집액.
