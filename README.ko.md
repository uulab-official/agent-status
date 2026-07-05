# Agent Status

**AI Agent Usage & Status Monitor** — 모든 AI 에이전트의 한도, 리셋 시간, 비용을 메뉴바 하나에서.

[English README](README.md) · [로드맵](ROADMAP.md) · [아키텍처](docs/architecture.md) · [기여 가이드](CONTRIBUTING.md)

> 현재 상태: **초기 스캐폴드 단계로, 아직 완성된 제품은 아닙니다.** 표준 모델,
> 플러그인 아키텍처, 트레이 아이콘, 팝오버, 설정 기능까지는 실제로 동작합니다.
> 대부분의 Provider는 자동 감지(`detect()`)까지만 되어 있고 실제 데이터 조회는
> 아직입니다. 정확한 진행 상황은 [ROADMAP.md](ROADMAP.md)를 기준으로 판단해 주세요.

## 문제의식

한도가 얼마나 남았는지 확인하려면, 매번 이렇게 해야 합니다.

```
Claude → 브라우저 접속 → Settings → Usage → 확인
GPT     → 동일, 다른 탭
Gemini  → 동일
Cursor  → 동일
```

## 무엇을 만드나

**Raycast + iStat Menus + Activity Monitor를 AI 서비스용으로 합친** macOS 메뉴바 /
Windows 트레이 유틸리티입니다. 항상 실행 중이며 모든 Provider의 사용량을 한눈에
보여줍니다.

```
🤖 72%              ← 컴팩트: 전체 Provider 중 가장 높은 사용률
🤖 C82 G41 O99      ← 상세: Provider별 이니셜 + 최고 사용률
```

클릭하면 세부 내역:

```
Claude   ███████░░░ 71%   2시간 14분 후 리셋
GPT      █████░░░░░ 53%   43분 후 리셋
Gemini   ████████░░ 82%   주간 41%
OpenRouter                이번 달 $12.42
```

이 클릭-오픈 팝오버, 트레이 라벨 자체, 설정 항목(메뉴바 모드, 로그인 시 시작)까지
전부 지금 실제로 동작합니다 — 무엇을 실행하고 검증했는지는
[src-tauri/README.md](src-tauri/README.md) 참고.

## Electron이 아니라 Rust + Tauri로 만든 이유

다른 프로그램들의 리소스 사용량을 감시하는 메뉴바 유틸리티가 정작 자기 자신이
가장 무거운 프로그램 중 하나가 되어서는 안 됩니다 — 작은 트레이 아이콘 하나를
위해 Chromium+Node 전체를 번들링하는 건 피할 수 있는 아이러니입니다. 이
프로젝트는 원래 TypeScript/Electron 프로토타입으로 시작했지만, 바로 이 이유로
첫 커밋 전에 Rust/Tauri로 다시 만들었습니다. 트레이드오프에 대한 자세한 논의는
[docs/architecture.md](docs/architecture.md#why-rust--tauri-not-electron) 참고.

## 왜 서버 기반 대시보드도 아닌가

각 AI 서비스의 사용량 API/화면은 예고 없이 자주 바뀝니다. 서버로 이를 중앙에서
추적하면, 모든 사용자를 위해 모든 벤더의 변경사항을 영원히 쫓아다녀야 합니다.
대신 Provider마다 독립된 플러그인이 있고, 각자가 관찰 가능한 것(API, CLI
로컬 상태, 로그인 세션 스크래핑)을 하나의 표준 모델로 변환합니다. 어떤 Provider가
망가지거나 정책이 바뀌어도 앱 전체가 아니라 작은 크레이트 하나만 고치면 됩니다.
자세한 내용은 [docs/architecture.md](docs/architecture.md) 참고.

## 지원 / 예정 Provider

| Provider | 상태 | 목표 신뢰도 |
|---|---|---|
| [Ollama](crates/providers/ollama) | ✅ 완전 구현 | ★★★★★ 공식 로컬 API |
| [Custom / OpenAI 호환](crates/providers/custom) (LM Studio, AnythingLLM, Open WebUI) | ✅ 완전 구현 | ★★★★★ |
| [OpenRouter](crates/providers/openrouter) | ✅ 완전 구현 | ★★★★★ API |
| [Codex](crates/providers/codex) | ✅ 실제 연결 상태 확인 | ★★★☆☆ CLI (`codex login status`) — 사용량 API 자체가 없음 |
| [Cursor](crates/providers/cursor) | ✅ 실제 연결 상태 확인 | ★★★☆☆ CLI (`cursor-agent status`); ★★★★☆ 대시보드 한도는 세션 쿠키가 필요해 의도적으로 보류 (README 참고) |
| [Claude](crates/providers/claude) | ✅ 완전 구현 | ★★★☆☆ CLI 로그(`~/.claude` 세션 트랜스크립트) — 토큰 수만, 플랜 한도 퍼센트는 없음 |
| [OpenAI / ChatGPT](crates/providers/openai) | ✅ 완전 구현 (플랫폼 API 비용) | ★★★★★ Admin Costs API — ChatGPT 플랜 메시지 한도(★★☆☆☆ 브라우저)는 아직 TODO |
| [Gemini](crates/providers/gemini) | ✅ 실제 연결 상태 확인 | ★★★★★ API 키 유효성 + 모델 목록 — 조회 가능한 사용량 엔드포인트 자체가 없음 |
| [GitHub Copilot](crates/providers/copilot) | 🚧 API 접근 막힘 — README 참고 | ★★★★★ API (v1.5) |

신뢰도(Confidence)는 부가 정보가 아니라 데이터 모델의 핵심 필드입니다.
이유는 [docs/confidence.md](docs/confidence.md), 🚧 상태인 Provider를 완성하는
방법은 [docs/plugin-development.md](docs/plugin-development.md)를 참고하세요.

## 저장소 구조

```
src-tauri/     Tauri 애플리케이션 — 트레이 아이콘, 팝오버 창, 스케줄러,
               그리고 모든 Provider를 조립하는 유일한 크레이트
ui/            정적 팝오버 프론트엔드 (HTML/CSS/vanilla JS, 빌드 단계 없음)
crates/
  core/          표준 상태 모델 + ProviderPlugin 트레이트
  plugins-common/ 모든 Provider가 공유하는 BasePluginState
  database/      SQLite 스키마 + 설정 저장 (rusqlite)
  notifications/  임계값 기반 알림 엔진
  tray-label/    순수 함수형 트레이 라벨 포맷팅
  providers/
    claude/ openai/ gemini/ cursor/ copilot/ codex/ ollama/ openrouter/ custom/
docs/          아키텍처, 데이터 모델, 신뢰도, 플러그인 개발 가이드
```

## 개발 환경 설정

Rust 툴체인(`rustup`)만 있으면 됩니다 — Node/npm 불필요.

```bash
cargo build --workspace
cargo test --workspace
```

실제 앱 실행 방법은 [src-tauri/README.md](src-tauri/README.md) 참고 —
macOS에서 트레이 아이콘을 수동 테스트하며 겪은 실제 함정(메뉴바 자동 숨김 +
합성 클릭 이슈)도 정리되어 있습니다.

## AI 코딩 에이전트를 위한 안내

이 저장소에서 작업하기 전에 [CLAUDE.md](CLAUDE.md) (Claude Code)와
[AGENTS.md](AGENTS.md) (Codex / 기타 에이전트)를 먼저 읽어주세요.

## 기여하기

[CONTRIBUTING.md](CONTRIBUTING.md) 참고. 지금 가장 임팩트가 큰 기여는 🚧
상태인 Provider 중 하나의 `fetch_status()`를 완성하는 것입니다 —
[docs/plugin-development.md](docs/plugin-development.md) 참고.

## 라이선스

[MIT](LICENSE)
