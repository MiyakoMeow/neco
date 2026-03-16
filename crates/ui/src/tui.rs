//! # 终端用户界面模块
//!
//! 本模块提供 `NeoCo` 的终端用户界面（TUI）实现。
//!
//! ## 主要组件
//!
//! - [`TuiInterface`]：TUI 接口实现
//! - [`TuiState`]：TUI 状态管理
//! - [`TuiMode`]：TUI 模式枚举

use crate::error::UiError;
use crate::traits::{AgentOutput, OutputType, UserInput, UserInterface};
use async_trait::async_trait;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal;
use neoco_agent::AgentEngine;
use neoco_context::ContextManager;
use neoco_core::ids::{AgentUlid, SessionUlid};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, List, ListState, Paragraph},
};
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

/// TUI 模式
///
/// 表示 TUI 的不同工作模式。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TuiMode {
    /// 普通模式
    Normal,
    /// 命令模式
    Command,
    /// 多行输入模式
    MultiLine,
    /// 命令面板模式
    CommandPalette,
}

/// 命令面板命令项
#[derive(Debug, Clone)]
pub struct CommandItem {
    /// 命令名称
    pub command: String,
    /// 命令描述
    pub description: String,
}

impl CommandItem {
    /// 创建新的命令项
    #[must_use]
    pub fn new(command: &str, description: &str) -> Self {
        Self {
            command: command.to_string(),
            description: description.to_string(),
        }
    }
}

/// 获取命令列表
#[must_use]
pub fn get_command_list() -> Vec<CommandItem> {
    vec![
        CommandItem::new("/new", "创建新会话"),
        CommandItem::new("/exit", "退出程序"),
        CommandItem::new("/compact", "压缩上下文"),
        CommandItem::new("/workflow status", "显示工作流状态"),
        CommandItem::new("/agents tree", "显示Agent树结构"),
    ]
}

/// TUI 状态
///
/// 存储 TUI 的会话和 Agent 状态信息。
#[derive(Debug, Clone)]
pub struct TuiState {
    /// 会话 ID
    pub session_id: SessionUlid,
    /// 根 Agent ID
    pub root_agent_id: AgentUlid,
    /// 当前 Agent ID
    pub current_agent_id: AgentUlid,
}

/// TUI 接口
///
/// 提供终端用户界面的完整实现。
pub struct TuiInterface {
    /// 终端实例
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
    /// 输入缓冲区
    input_buffer: String,
    /// 输出历史记录
    output_history: VecDeque<AgentOutput>,
    /// 最大历史记录大小
    max_history_size: usize,
    /// 当前模式
    mode: TuiMode,
    /// TUI 状态
    state: Arc<RwLock<Option<TuiState>>>,
    /// Agent 引擎
    engine: Arc<AgentEngine>,
    /// 上下文管理器（用于压缩功能）
    context_manager: Option<Arc<dyn ContextManager>>,
    /// 待恢复的 Session (`session_id`, `root_agent_id`)
    restore_session: Option<(SessionUlid, AgentUlid)>,
    /// 输入请求发送通道
    input_tx: Arc<RwLock<Option<mpsc::Sender<UserInput>>>>,
    /// 输入响应接收通道
    input_rx: Arc<RwLock<Option<mpsc::Receiver<UserInput>>>>,
    /// 命令面板选择状态
    command_list_state: ListState,
    /// 命令列表
    command_list: Vec<CommandItem>,
}

impl TuiInterface {
    /// 创建新的 TUI 接口实例
    ///
    /// # 参数
    ///
    /// * `max_history_size` - 最大历史记录大小
    /// * `engine` - Agent 引擎
    /// * `context_manager` - 上下文管理器（可选，用于压缩功能）
    /// * `session_manager` - Session 管理器（可选，用于恢复会话）
    /// * `restore_session_ulid` - 待恢复的 Session ULID（可选）
    ///
    /// # Errors
    ///
    /// 如果终端初始化失败，返回错误
    ///
    /// # 返回
    ///
    /// 返回 TUI 接口实例或错误
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_history_size: usize,
        engine: Arc<AgentEngine>,
        context_manager: Option<Arc<dyn ContextManager>>,
        restore_session: Option<(SessionUlid, AgentUlid)>,
    ) -> Result<Self, UiError> {
        let terminal =
            Terminal::new(CrosstermBackend::new(io::stderr())).map_err(UiError::Terminal)?;
        let command_list = get_command_list();
        Ok(Self {
            terminal,
            input_buffer: String::new(),
            output_history: VecDeque::new(),
            max_history_size,
            mode: TuiMode::Normal,
            state: Arc::new(RwLock::new(None)),
            engine,
            context_manager,
            restore_session,
            input_tx: Arc::new(RwLock::new(None)),
            input_rx: Arc::new(RwLock::new(None)),
            command_list_state: ListState::default(),
            command_list,
        })
    }

    /// 初始化会话
    ///
    /// 创建并初始化一个新的会话。
    ///
    /// # 参数
    ///
    /// * `session_id` - 会话 ID
    /// * `root_agent_id` - 根 Agent ID
    ///
    /// # Errors
    ///
    /// 如果会话初始化失败，返回错误
    ///
    /// # 返回
    ///
    /// 成功时返回 Ok(())，失败时返回错误
    pub async fn init_session(
        &self,
        session_id: SessionUlid,
        root_agent_id: AgentUlid,
    ) -> Result<(), UiError> {
        let mut state = self.state.write().await;
        *state = Some(TuiState {
            session_id,
            root_agent_id,
            current_agent_id: root_agent_id,
        });
        Ok(())
    }

    /// 处理命令
    async fn handle_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<AgentOutput, UiError> {
        match name {
            "new" => self.cmd_new().await,
            "compact" => self.cmd_compact(args).await,
            "workflow" => self.cmd_workflow(args).await,
            "agents" => self.cmd_agents(args).await,
            _ => Ok(AgentOutput {
                content: format!("未知命令: /{name}"),
                output_type: OutputType::Error,
            }),
        }
    }

    /// 创建新会话命令
    async fn cmd_new(&self) -> Result<AgentOutput, UiError> {
        let session_id = SessionUlid::new();
        let root_agent_id = AgentUlid::new_root(&session_id);

        self.init_session(session_id, root_agent_id).await?;

        Ok(AgentOutput {
            content: format!(
                "已创建新Session\n  Session ID: {session_id}\n  Root Agent ID: {root_agent_id}"
            ),
            output_type: OutputType::Text,
        })
    }

    /// 压缩上下文命令
    async fn cmd_compact(&mut self, _args: &[String]) -> Result<AgentOutput, UiError> {
        let state = self.state.read().await;
        if let Some(ref s) = *state {
            if let Some(ref cm) = self.context_manager {
                let confirm_msg =
                    "请确认压缩上下文？这将合并旧消息以节省token。\n输入 'y' 确认或 'n' 取消: ";

                let (tx, rx) = tokio::sync::mpsc::channel::<UserInput>(1);
                {
                    let mut tx_guard = self.input_tx.write().await;
                    *tx_guard = Some(tx);
                }
                {
                    let mut rx_guard = self.input_rx.write().await;
                    *rx_guard = Some(rx);
                }

                self.output_history.push_back(AgentOutput {
                    content: confirm_msg.to_string(),
                    output_type: OutputType::Text,
                });

                let user_input: Option<UserInput> = tokio::select! {
                    input = async {
                        let mut rx_guard = self.input_rx.write().await;
                        if let Some(rx) = rx_guard.as_mut() {
                            rx.recv().await
                        } else {
                            None
                        }
                    } => input,
                };

                {
                    let mut tx_guard = self.input_tx.write().await;
                    *tx_guard = None;
                }
                {
                    let mut rx_guard = self.input_rx.write().await;
                    *rx_guard = None;
                }

                let confirmed = match user_input {
                    Some(UserInput::Message(msg)) => msg.trim().to_lowercase() == "y",
                    _ => false,
                };

                if !confirmed {
                    return Ok(AgentOutput {
                        content: "已取消上下文压缩".to_string(),
                        output_type: OutputType::Text,
                    });
                }

                match cm.compact(&s.current_agent_id, None).await {
                    Ok(result) => Ok(AgentOutput {
                        content: format!(
                            "上下文压缩成功！\n  原始消息数: {}\n  压缩后消息数: {}\n  Token节省: {} ({:.1}%)",
                            result.original_count,
                            result.compacted_count,
                            result.token_savings.saved,
                            result.token_savings.saved_percent
                        ),
                        output_type: OutputType::Text,
                    }),
                    Err(e) => Ok(AgentOutput {
                        content: format!("上下文压缩失败: {e}"),
                        output_type: OutputType::Error,
                    }),
                }
            } else {
                Ok(AgentOutput {
                    content: "错误: 压缩服务未配置".to_string(),
                    output_type: OutputType::Error,
                })
            }
        } else {
            Ok(AgentOutput {
                content: "错误: 没有活动的Session，请先使用 /new 创建新Session".to_string(),
                output_type: OutputType::Error,
            })
        }
    }

    /// 工作流命令
    async fn cmd_workflow(&self, args: &[String]) -> Result<AgentOutput, UiError> {
        let state = self.state.read().await;
        if let Some(ref s) = *state {
            if args.first().is_some_and(|a| a == "status") {
                Ok(AgentOutput {
                    content: format!(
                        "工作流状态\n  Session ID: {}\n  Root Agent: {}\n  当前Agent: {}\n  状态: 活跃",
                        s.session_id, s.root_agent_id, s.current_agent_id
                    ),
                    output_type: OutputType::Text,
                })
            } else {
                Ok(AgentOutput {
                    content: "用法: /workflow status - 显示工作流状态".to_string(),
                    output_type: OutputType::Text,
                })
            }
        } else {
            Ok(AgentOutput {
                content: "错误: 没有活动的Session，请先使用 /new 创建新Session".to_string(),
                output_type: OutputType::Error,
            })
        }
    }

    /// Agent 管理命令
    async fn cmd_agents(&self, args: &[String]) -> Result<AgentOutput, UiError> {
        let state = self.state.read().await;
        if let Some(ref s) = *state {
            if args.first().is_some_and(|a| a == "tree") {
                let agent_repo = self.engine.agent_repo();
                let tree_output = self
                    .build_agent_tree_output(&agent_repo, s.root_agent_id, 0)
                    .await
                    .unwrap_or_else(|e| format!("Error: {e}"));

                Ok(AgentOutput {
                    content: tree_output,
                    output_type: OutputType::Text,
                })
            } else {
                Ok(AgentOutput {
                    content: "用法: /agents tree - 显示Agent树结构".to_string(),
                    output_type: OutputType::Text,
                })
            }
        } else {
            Ok(AgentOutput {
                content: "错误: 没有活动的Session，请先使用 /new 创建新Session".to_string(),
                output_type: OutputType::Error,
            })
        }
    }

    /// 构建 Agent 树输出
    async fn build_agent_tree_output(
        &self,
        agent_repo: &Arc<dyn neoco_core::traits::AgentRepository>,
        root_agent_id: AgentUlid,
        _depth: usize,
    ) -> Result<String, neoco_core::AgentError> {
        let mut output = String::new();
        let mut stack: Vec<(AgentUlid, usize)> = vec![(root_agent_id, 0)];

        while let Some((agent_id, depth)) = stack.pop() {
            let agent = agent_repo.find_by_id(&agent_id).await?;
            let _ = agent.ok_or_else(|| neoco_core::AgentError::NotFound(agent_id.to_string()))?;

            let indent = "  ".repeat(depth);
            #[allow(clippy::format_push_string)]
            output.push_str(&format!("{indent}Agent: {agent_id} (type: default)\n"));

            let children = agent_repo.find_children(&agent_id).await?;
            for child in children.into_iter().rev() {
                stack.push((child.id, depth + 1));
            }
        }

        Ok(output)
    }

    /// 运行 TUI
    ///
    /// 启动 TUI 主循环，处理用户输入和渲染输出。
    ///
    /// # Errors
    ///
    /// 如果终端初始化失败，返回错误
    /// 如果运行循环中出现错误，返回错误
    ///
    /// # 返回
    ///
    /// 成功时返回 Ok(())，失败时返回错误
    pub async fn run(&mut self) -> Result<(), UiError> {
        terminal::enable_raw_mode().map_err(UiError::Terminal)?;

        let result = self.run_loop().await;

        let _ = terminal::disable_raw_mode();

        result
    }

    /// 运行 TUI 主循环
    async fn run_loop(&mut self) -> Result<(), UiError> {
        loop {
            let input_buffer = self.input_buffer.clone();
            let output_history = self.output_history.clone();
            let mode = self.mode;
            let command_list = self.command_list.clone();
            let mut command_list_state = self.command_list_state.clone();

            self.terminal
                .draw(|f| {
                    draw_frame(
                        f,
                        &input_buffer,
                        &output_history,
                        mode,
                        &command_list,
                        &mut command_list_state,
                    );
                })
                .map_err(UiError::Terminal)?;

            self.command_list_state = command_list_state;

            if event::poll(std::time::Duration::from_millis(16)).map_err(UiError::Terminal)? {
                match event::read().map_err(UiError::Terminal)? {
                    #[allow(clippy::match_same_arms)]
                    Event::Resize(_, _) => {
                        let size = self.terminal.size().unwrap_or_default();
                        let rect = Rect::new(0, 0, size.width, size.height);
                        self.terminal.resize(rect).map_err(UiError::Terminal)?;
                    },
                    Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Char('p')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            self.mode = TuiMode::CommandPalette;
                            self.command_list_state.select_first();
                        },
                        KeyCode::Char('c')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            return Ok(());
                        },
                        KeyCode::Down | KeyCode::Char('j')
                            if self.mode == TuiMode::CommandPalette =>
                        {
                            let selected = self.command_list_state.selected();
                            if let Some(idx) = selected {
                                if idx < self.command_list.len() - 1 {
                                    self.command_list_state.select(Some(idx + 1));
                                }
                            } else {
                                self.command_list_state.select_first();
                            }
                        },
                        KeyCode::Up | KeyCode::Char('k')
                            if self.mode == TuiMode::CommandPalette =>
                        {
                            let selected = self.command_list_state.selected();
                            if let Some(idx) = selected {
                                if idx > 0 {
                                    self.command_list_state.select(Some(idx - 1));
                                }
                            } else {
                                self.command_list_state.select_last();
                            }
                        },
                        KeyCode::Enter if self.mode == TuiMode::CommandPalette => {
                            if let Some(idx) = self.command_list_state.selected()
                                && let Some(cmd) = self.command_list.get(idx)
                            {
                                let input = cmd.command.clone();
                                self.mode = TuiMode::Normal;
                                self.input_buffer = input;
                            }
                        },
                        KeyCode::Esc if self.mode == TuiMode::CommandPalette => {
                            self.mode = TuiMode::Normal;
                            self.command_list_state.select(None);
                        },
                        KeyCode::Enter => {
                            if !self.input_buffer.is_empty() {
                                let input = self.input_buffer.clone();
                                self.input_buffer.clear();
                                if let Some(stripped) = input.strip_prefix('/') {
                                    let parts: Vec<&str> = stripped.splitn(2, ' ').collect();
                                    let name = parts.first().copied().unwrap_or("").to_string();
                                    let args: Vec<String> = parts
                                        .get(1)
                                        .map(|s| s.split(' ').map(String::from).collect())
                                        .unwrap_or_default();

                                    if name == "exit" {
                                        return Ok(());
                                    }

                                    let result = self.handle_command(&name, &args).await;
                                    let output = match result {
                                        Ok(o) => o,
                                        Err(e) => AgentOutput {
                                            content: format!("命令执行错误: {e}"),
                                            output_type: OutputType::Error,
                                        },
                                    };
                                    self.output_history.push_back(output);
                                } else {
                                    self.output_history.push_back(AgentOutput {
                                        content: format!("User: {input}"),
                                        output_type: OutputType::Text,
                                    });

                                    let state = self.state.read().await;
                                    if let Some(ref s) = *state {
                                        let result = self
                                            .engine
                                            .run_agent(s.current_agent_id, input.clone())
                                            .await;
                                        let response = match result {
                                            Ok(agent_result) => agent_result.output,
                                            Err(e) => format!("Agent error: {e}"),
                                        };
                                        self.output_history.push_back(AgentOutput {
                                            content: response,
                                            output_type: OutputType::Text,
                                        });
                                    } else {
                                        self.output_history.push_back(AgentOutput {
                                            content:
                                                "Error: No active session. Use /new to create one."
                                                    .to_string(),
                                            output_type: OutputType::Error,
                                        });
                                    }
                                }
                                if self.output_history.len() > self.max_history_size {
                                    self.output_history.pop_front();
                                }
                            }
                        },
                        KeyCode::Char(c) => {
                            if c == '/' {
                                self.mode = TuiMode::Command;
                            }
                            self.input_buffer.push(c);
                        },
                        KeyCode::Backspace => {
                            self.input_buffer.pop();
                        },
                        KeyCode::Esc => {
                            self.mode = TuiMode::Normal;
                            self.input_buffer.clear();
                        },
                        _ => {},
                    },
                    _ => {},
                }
            }
        }
    }
}

/// 绘制 TUI 帧
fn draw_frame(
    f: &mut Frame,
    input_buffer: &str,
    output_history: &VecDeque<AgentOutput>,
    mode: TuiMode,
    command_list: &[CommandItem],
    command_list_state: &mut ListState,
) {
    if mode == TuiMode::CommandPalette {
        draw_command_palette(f, command_list, command_list_state);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    let messages: Vec<Line> = output_history
        .iter()
        .map(|o| Line::from(o.content.as_str()))
        .collect();

    let messages_widget = Paragraph::new(messages)
        .block(Block::bordered().title("Messages"))
        .style(Style::default().fg(Color::Gray));

    if let Some(chunk) = chunks.first() {
        f.render_widget(messages_widget, *chunk);
    }

    let input_widget = Paragraph::new(input_buffer)
        .block(Block::bordered().title("Input"))
        .style(Style::default().fg(Color::White));

    if let Some(chunk) = chunks.get(1) {
        f.render_widget(input_widget, *chunk);
    }

    let status = match mode {
        TuiMode::Normal => "Normal Mode",
        TuiMode::Command => "Command Mode",
        TuiMode::MultiLine => "Multi-line Mode",
        TuiMode::CommandPalette => "Command Palette Mode",
    };
    let status_widget = Paragraph::new(status).style(Style::default().fg(Color::Cyan));
    if let Some(chunk) = chunks.get(2) {
        f.render_widget(status_widget, *chunk);
    }
}

/// 绘制命令面板
fn draw_command_palette(
    f: &mut Frame,
    command_list: &[CommandItem],
    command_list_state: &mut ListState,
) {
    let area = f.area();
    let width = std::cmp::min(50, area.width.saturating_sub(4));
    let height = std::cmp::min(
        u16::try_from(command_list.len()).unwrap_or(u16::MAX) + 2,
        area.height.saturating_sub(4),
    );

    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    let popup_area = ratatui::layout::Rect::new(
        x.saturating_sub(1),
        y.saturating_sub(1),
        x + width + 1,
        y + height + 1,
    );

    let items: Vec<ratatui::text::Text> = command_list
        .iter()
        .map(|cmd| ratatui::text::Text::from(format!("{} - {}", cmd.command, cmd.description)))
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title("命令面板 (Ctrl+p 打开, Enter 执行, Esc 关闭)"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, popup_area, command_list_state);
}

#[async_trait]
impl UserInterface for TuiInterface {
    async fn init(&mut self) -> Result<(), UiError> {
        if let Some((session_id, root_agent_id)) = self.restore_session.take() {
            self.init_session(session_id, root_agent_id).await?;
            self.output_history.push_back(AgentOutput {
                content: format!(
                    "已恢复会话\n  Session ID: {session_id}\n  Root Agent ID: {root_agent_id}\n输入消息开始对话，或输入 /help 查看命令"
                ),
                output_type: OutputType::Text,
            });
            return Ok(());
        }

        let session_id = SessionUlid::new();
        let root_agent_id = AgentUlid::new_root(&session_id);
        self.init_session(session_id, root_agent_id).await?;
        self.output_history.push_back(AgentOutput {
            content: format!(
                "已创建新Session\n  Session ID: {session_id}\n  Root Agent ID: {root_agent_id}\n输入消息开始对话，或输入 /help 查看命令"
            ),
            output_type: OutputType::Text,
        });
        Ok(())
    }

    async fn get_input(&mut self) -> Result<UserInput, UiError> {
        let (tx, rx) = mpsc::channel::<UserInput>(1);

        {
            let mut tx_guard = self.input_tx.write().await;
            *tx_guard = Some(tx);
        }
        {
            let mut rx_guard = self.input_rx.write().await;
            *rx_guard = Some(rx);
        }

        let input = loop {
            let input_buffer = self.input_buffer.clone();
            let output_history = self.output_history.clone();
            let mode = self.mode;
            let command_list = vec![];
            let mut command_list_state = ListState::default();

            self.terminal
                .draw(|f| {
                    draw_frame(
                        f,
                        &input_buffer,
                        &output_history,
                        mode,
                        &command_list,
                        &mut command_list_state,
                    );
                })
                .map_err(UiError::Terminal)?;

            if event::poll(std::time::Duration::from_millis(16)).map_err(UiError::Terminal)?
                && let Event::Key(key) = event::read().map_err(UiError::Terminal)?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('c')
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        {
                            let mut tx_guard = self.input_tx.write().await;
                            *tx_guard = None;
                        }
                        {
                            let mut rx_guard = self.input_rx.write().await;
                            *rx_guard = None;
                        }
                        return Ok(UserInput::Interrupt);
                    },
                    KeyCode::Enter => {
                        if !self.input_buffer.is_empty() {
                            let input = self.input_buffer.clone();
                            self.input_buffer.clear();
                            break UserInput::Message(input);
                        }
                    },
                    KeyCode::Char(c) => {
                        if c == '/' {
                            self.mode = TuiMode::Command;
                        }
                        self.input_buffer.push(c);
                    },
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    },
                    KeyCode::Esc => {
                        self.mode = TuiMode::Normal;
                        self.input_buffer.clear();
                    },
                    _ => {},
                }
            }
        };

        {
            let mut tx_guard = self.input_tx.write().await;
            *tx_guard = None;
        }
        {
            let mut rx_guard = self.input_rx.write().await;
            *rx_guard = None;
        }

        Ok(input)
    }

    async fn render(&mut self, output: &AgentOutput) -> Result<(), UiError> {
        if self.output_history.len() >= self.max_history_size {
            self.output_history.pop_front();
        }
        self.output_history.push_back(output.clone());
        Ok(())
    }

    async fn ask(
        &mut self,
        question: &str,
        options: Option<Vec<String>>,
    ) -> Result<String, UiError> {
        let options = options.unwrap_or_default();

        if options.is_empty() {
            return Ok(String::new());
        }

        let question_with_options = format!(
            "{}\n{}\n请输入选项编号 (1-{}): ",
            question,
            options
                .iter()
                .enumerate()
                .map(|(i, opt)| format!("  {}. {}", i + 1, opt))
                .collect::<Vec<_>>()
                .join("\n"),
            options.len()
        );

        self.output_history.push_back(AgentOutput {
            content: question_with_options.clone(),
            output_type: OutputType::Text,
        });

        let (tx, rx) = mpsc::channel::<UserInput>(1);

        {
            let mut tx_guard = self.input_tx.write().await;
            *tx_guard = Some(tx);
        }
        {
            let mut rx_guard = self.input_rx.write().await;
            *rx_guard = Some(rx);
        }

        let input = loop {
            let input_buffer = self.input_buffer.clone();
            let output_history = self.output_history.clone();
            let mode = self.mode;
            let command_list = vec![];
            let mut command_list_state = ListState::default();

            self.terminal
                .draw(|f| {
                    draw_frame(
                        f,
                        &input_buffer,
                        &output_history,
                        mode,
                        &command_list,
                        &mut command_list_state,
                    );
                })
                .map_err(UiError::Terminal)?;

            if event::poll(std::time::Duration::from_millis(16)).map_err(UiError::Terminal)?
                && let Event::Key(key) = event::read().map_err(UiError::Terminal)?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('c')
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        {
                            let mut tx_guard = self.input_tx.write().await;
                            *tx_guard = None;
                        }
                        {
                            let mut rx_guard = self.input_rx.write().await;
                            *rx_guard = None;
                        }
                        return Err(UiError::BadRequest("用户取消输入".to_string()));
                    },
                    KeyCode::Enter => {
                        if !self.input_buffer.is_empty() {
                            let input = self.input_buffer.clone();
                            self.input_buffer.clear();
                            break UserInput::Message(input);
                        }
                    },
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                    },
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                    },
                    KeyCode::Esc => {
                        self.input_buffer.clear();
                    },
                    _ => {},
                }
            }
        };

        {
            let mut tx_guard = self.input_tx.write().await;
            *tx_guard = None;
        }
        {
            let mut rx_guard = self.input_rx.write().await;
            *rx_guard = None;
        }

        match input {
            UserInput::Message(msg) => {
                if let Ok(num) = msg.trim().parse::<usize>() {
                    if let Some(opt) = options.get(num - 1) {
                        Ok(opt.clone())
                    } else {
                        Ok(String::new())
                    }
                } else {
                    if let Some(idx) = options
                        .iter()
                        .position(|opt| opt.eq_ignore_ascii_case(msg.trim()))
                    {
                        if let Some(opt) = options.get(idx) {
                            Ok(opt.clone())
                        } else {
                            Ok(String::new())
                        }
                    } else {
                        Ok(String::new())
                    }
                }
            },
            _ => Ok(String::new()),
        }
    }

    async fn shutdown(&mut self) -> Result<(), UiError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_mode_variants() {
        assert_eq!(TuiMode::Normal, TuiMode::Normal);
        assert_eq!(TuiMode::Command, TuiMode::Command);
        assert_ne!(TuiMode::Normal, TuiMode::Command);
    }

    #[test]
    fn test_output_type_variants() {
        let _ = OutputType::Text;
        let _ = OutputType::Markdown;
        let _ = OutputType::Code {
            language: "rust".to_string(),
        };
        let _ = OutputType::ToolResult {
            tool_name: "bash".to_string(),
        };
        let _ = OutputType::Error;
    }
}
