# devnet 배포 기록

| 시각(UTC) | 커밋 | 배포 슬롯 | 바이너리 sha256 앞 16자 | 비고 |
|---|---|---|---|---|
| 2026-11-27T09:52:44Z | 08b1112 | 526829019 | ffba5ccc26aedb80 | P1 |


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

## P1 devnet 실행 (2026-11-27, 워크플로 실행 45976185555)

프로그램 [`94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn`](https://explorer.solana.com/address/94CLe2FNJ9ZHUq4i8pRARSRkkEvEMkL4fBrCcJg2hcHn?cluster=devnet)을 P1 바이너리(커밋 `08b1112`, sha256 앞 16자 `ffba5ccc26aedb80`)로 다시 올린 뒤 `scripts/run/p1.ts devnet`을 돌렸다(24건 성공, 실패해야 하는 4건은 사전 시뮬레이션 없이 보내 체인에 실패로 남김, 92초). 같은 실행에서 devnet의 Token ACL·ABL 게이트 바이너리를 내려받아 P1 LiteSVM 테스트 13건을 그 바이너리로도 돌렸고 모두 통과했다(저장소 고정 바이너리로도 13/13). 기록은 브랜치 `devnet-p1-45976185555`, 요약은 `app/public/p1-run.json`.

| 항목 | 주소 | 확인한 값 |
|---|---|---|
| Token ACL / ABL 게이트(devnet 배포본) | [`TACLkU6C…`](https://explorer.solana.com/address/TACLkU6CiCdkQN2MjoyDkVg2yAH9zkxiHDsiztQ52TP?cluster=devnet) / [`GATEzzqx…`](https://explorer.solana.com/address/GATEzzqxhJnsWF6vHRsgtixxSB8PaQdcqGEVTEHWiULz?cluster=devnet) | 실행 가능 계정, 내려받은 바이너리 sha256 앞 16자 `bd467aa222517ac7` / `88a03cc277037bc0` |
| 계약(Deal, 모집 단계) | [`AsR48hi5…`](https://explorer.solana.com/address/AsR48hi5oRyXY271PXd1Nc68SfqZvnhT8t9JVJSGjfSz?cluster=devnet) | `tranche_mints` = 아래 두 민트 |
| 선순위 민트 | [`Fpa2Ne4G…`](https://explorer.solana.com/address/Fpa2Ne4GMXC3FKcXfPZHyekVyy8XQF2zdkeQKQLEZxjC?cluster=devnet) | 확장 MetadataPointer, TransferHook, DefaultAccountState, TokenMetadata, DefaultAccountState **Frozen**, 메타데이터 `token_acl` = 게이트, 훅 = SaveSail(변경 권한 없음), 발행 권한 = Deal PDA, 공급 10,000 |
| 선순위 Token ACL MintConfig | [`EnjuDFYa…`](https://explorer.solana.com/address/EnjuDFYaApJtHB8tZxTqzVCKAFihT4x2y7nH7aFkiUmk?cluster=devnet) | 게이트 프로그램 = ABL 게이트, 무허가 동결 해제 켜짐, 민트의 동결 권한 = 이 계정 |
| 후순위 민트 | [`EJ9MDd8p…`](https://explorer.solana.com/address/EJ9MDd8pS4KhvDMovh5PXVfMWi8vmUo2VHnrgGJHYer?cluster=devnet) | 확장 MetadataPointer, TransferHook, DefaultAccountState, TokenMetadata, DefaultAccountState **Frozen**, 메타데이터 `token_acl` = 게이트, 훅 = SaveSail(변경 권한 없음), 발행 권한 = Deal PDA, 공급 0 |
| 후순위 Token ACL MintConfig | [`AoMjvhLh…`](https://explorer.solana.com/address/AoMjvhLh7155KVHNt6v52KNesYUkktGGiXNSThWFR1FZ?cluster=devnet) | 게이트 프로그램 = ABL 게이트, 무허가 동결 해제 켜짐, 민트의 동결 권한 = 이 계정 |
| ABL 허용 목록 | [`C6jSheks…`](https://explorer.solana.com/address/C6jSheksL3qCTPDoVkbsXaNhX7dG4CDBifehttbrijGW?cluster=devnet) | 모드 allow, 지갑 2개(투자자 A [`CGW2M9jN…`](https://explorer.solana.com/address/CGW2M9jN8jR39FfiWX5KTGxZGT3xF4QnVTJnSPPFBRs1?cluster=devnet), B [`GpRVeiPg…`](https://explorer.solana.com/address/GpRVeiPgB5FTb4g21hJDKFNfgiwNwifwh85zCq41AvVN?cluster=devnet)) |

| 보유자 | 목록 | 선순위 토큰 계정 | 만들 때 | 지금 | 잔액 = 포지션 단위 |
|---|---|---|---|---|---|
| 투자자 A | 있음 | [`CorSmV8s…`](https://explorer.solana.com/address/CorSmV8s9hiy81tJHuSJUBUDMnR5j7cqo4UvizPwjPQ2?cluster=devnet) | 동결 | 해제 | 7,500 = 7,500 |
| 투자자 B | 있음 | [`4koonxMt…`](https://explorer.solana.com/address/4koonxMtLQ9apwT4KUzoVKrQuMdYVFsdsXGYV446W8SC?cluster=devnet) | 동결 | 해제 | 2,500 = 2,500 |
| KYC 통과·목록 밖 지갑 | 없음 | [`BYwGQhVj…`](https://explorer.solana.com/address/BYwGQhVjNqpbo9rTQMQogTKyjS5NoySGqtdrF5snDmU5?cluster=devnet) | 동결 | 동결 | 0 = 0 |

| 단계 | 결과 | 트랜잭션 |
|---|---|---|
| ABL 목록 생성 | 성공 | [`3CxcKpHZfQLQNA2p…`](https://explorer.solana.com/tx/3CxcKpHZfQLQNA2pVDFMjDbrUb6zVMdTg94bfFWkMGCFovrHoXXAN7bAAWM3Ubjwy8WXsdvCCAhpwYEwdKkr7xXZ?cluster=devnet) |
| 선순위 민트 생성(확장·메타데이터·발행 권한 이전) | 성공 | [`2xao4MJ4XXRgN19m…`](https://explorer.solana.com/tx/2xao4MJ4XXRgN19mTNfLFo5Y1TwHqz9uwUQECfvizXgXarBh4dRGzjSmp6FhVeqsadwCnPxoDoKYkXuTTsLrCT6R?cluster=devnet) |
| Token ACL MintConfig·무허가 해제·게이트 메타 | 성공 | [`5qCWTrDakeKbYYrL…`](https://explorer.solana.com/tx/5qCWTrDakeKbYYrLMaLcsVuXBBXrkWbkUQxe6bR7EvLkCaD9hTzGkqduWdy5nuQ4vvTFmRxo3W1SMijzWE6PnUgQ?cluster=devnet) |
| 트랜치 민트 등록(규칙 검사) | 성공 | [`5aEgf6vmHqu6VYYg…`](https://explorer.solana.com/tx/5aEgf6vmHqu6VYYgzggLJyW9863kTtThe3iszaDAAHivc8CvskgTv79FxAVBwFkSWZRdmKEGRHyUZrSgTA863jXE?cluster=devnet) |
| 투자자 A 스스로 동결 해제 | 성공 | [`5ddVCqViXJw5vEXB…`](https://explorer.solana.com/tx/5ddVCqViXJw5vEXBUKiuhAGmkKUvoEPvmMrK98VBkwVb9R5vtD3R6nzoG4PPmGUsGNcDgdJS8hBx1siHLziFSuoi?cluster=devnet) |
| 투자자 A 투자(10,000 dUSD → 선순위 토큰 10,000) | 성공 | [`ECpCnEK1NE9SzAJM…`](https://explorer.solana.com/tx/ECpCnEK1NE9SzAJMiLwfqfMTokLtqbzgmptecr5RyPrBKr7UyvLQQE5ooWFdmzEYyN1ceVocnSkdY6QftiySNUp?cluster=devnet) |
| 투자자 B 스스로 동결 해제 | 성공 | [`3KShFZBF2umjSKUk…`](https://explorer.solana.com/tx/3KShFZBF2umjSKUk2FeafXVHt3pzRoAm1ucKrR4Cyrid2j5FkbPKWKuoBoZUBZxE6cUceUVw1zWu6g9yp3EFKvyQ?cluster=devnet) |
| A → B 이전 2,500(Transfer Hook 정산) | 성공 | [`4RhbJ76TbjYsxatc…`](https://explorer.solana.com/tx/4RhbJ76TbjYsxatcD9bDyZohheYkv97VKm5pftK3pXqBXrQrg5JRAqPQsjNoJXxbnWGsdFVJAfS5wjZEhUWLfb1y?cluster=devnet) |
| 목록 밖 지갑 스스로 동결 해제 | 실패: 게이트 `AccountBlocked`(4098) | [`4ZCwP7DBGcNfrQ4F…`](https://explorer.solana.com/tx/4ZCwP7DBGcNfrQ4FhKnr2VjqUGWeQY2kHee2JLba6NvtKXKxwoFUepAeGRzQM3Q7f1V6TtpLr4G4aeGAi7ukRG5P?cluster=devnet) |
| 목록 지갑(A)이 목록 밖 지갑 해제 시도 | 실패: 게이트 `AccountBlocked`(4098) | [`AHaqHWAhRShKPz2x…`](https://explorer.solana.com/tx/AHaqHWAhRShKPz2xPf3JqeyyLq4eF6RXnzMcP364VGMcfKvWKidssPszJnDYKS2ZZQeEN1UedpLtM4ZYGrrDA9c?cluster=devnet) |
| 목록 밖 지갑 투자(발행 수령) | 실패: Token-2022 `Account is frozen` | [`5VUujsqiCaE2Becn…`](https://explorer.solana.com/tx/5VUujsqiCaE2BecnLrRhzcGMDhqWN4ihef1V9gukmogXrNs7ifmMYe1oYaYyH9C1eY5G7wjhZewKbQHm95UCr42i?cluster=devnet) |
| A → 목록 밖 지갑 이전(수령) | 실패: Token-2022 `Account is frozen` | [`mcKRX2YbMVFVrfmS…`](https://explorer.solana.com/tx/mcKRX2YbMVFVrfmSxsYm8mAx9WKBQuUEQZQ9grRUMBTRfeGmb9rgXq7gV9FVnMHDaqhFx4AC5K61UmpgcJgGiH3?cluster=devnet) |

점검 11/11 통과: 트랜치마다 민트 1개, 기본 동결, `token_acl` 메타데이터, 훅 = SaveSail·변경 불가, MintConfig 게이트·무허가 해제, 허용 목록, 새 계정 동결 생성, 목록 지갑 해제·수령, 목록 밖 지갑 해제·수령 실패, 단위 = 잔액, 공급 = 모집액.
