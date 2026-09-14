# Paperclip / Parrot Component Inventory

自动生成：`scripts/generate_parity_inventory.py`。

该清单用于 M0 的结构差异定位；不能替代 API 行为、权限、Schema、E2E 或视觉验收。

## CLI

CLI 归属已固定为 `parrot-agent/crates/cli`；后续需逐命令核对参数、认证、退出码和输出。

- Paperclip: **97**
- Parrot: **14**

| Paperclip evidence | Parrot evidence |
|---|---|
| `src/adapters/http/format-event.ts` | `Cargo.toml` |
| `src/adapters/http/index.ts` | `src/backup.rs` |
| `src/adapters/index.ts` | `src/bin/parrot.rs` |
| `src/adapters/process/format-event.ts` | `src/checks.rs` |
| `src/adapters/process/index.ts` | `src/client.rs` |
| `src/adapters/registry.ts` | `src/commands.rs` |
| `src/checks/agent-jwt-secret-check.ts` | `src/config.rs` |
| `src/checks/config-check.ts` | `src/install_store.rs` |
| `src/checks/database-check.ts` | `src/lib.rs` |
| `src/checks/deployment-auth-check.ts` | `src/plugin_scaffold.rs` |
| `src/checks/index.ts` | `src/services.rs` |
| `src/checks/llm-check.ts` | `src/update_notice.rs` |
| `src/checks/log-check.ts` | `tests/cli_e2e_import_export_doctor_test.rs` |
| `src/checks/managed-install-check.ts` | `tests/cli_http_parity_test.rs` |
| `src/checks/path-resolver.ts` | *(no structural counterpart in this slice)* |
| `src/checks/port-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/secrets-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/service-health-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/storage-check.ts` | *(no structural counterpart in this slice)* |
| `src/client/board-auth.ts` | *(no structural counterpart in this slice)* |
| `src/client/command-label.ts` | *(no structural counterpart in this slice)* |
| `src/client/context.ts` | *(no structural counterpart in this slice)* |
| `src/client/http.ts` | *(no structural counterpart in this slice)* |
| `src/commands/allowed-hostname.ts` | *(no structural counterpart in this slice)* |
| `src/commands/auth-bootstrap-ceo.ts` | *(no structural counterpart in this slice)* |
| `src/commands/channels.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/access.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/activity.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/adapter.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/agent.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/approval.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/asset.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/auth.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/common.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/company.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/connect.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/context.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/cost.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/dashboard.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/feedback.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/goal.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/issue.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/plugin.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/project.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/prompt.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/routine-api.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/run.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/secrets.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/skill.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/skills.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/teams.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/token.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/workspace.ts` | *(no structural counterpart in this slice)* |
| `src/commands/client/zip.ts` | *(no structural counterpart in this slice)* |
| `src/commands/configure.ts` | *(no structural counterpart in this slice)* |
| `src/commands/db-backup.ts` | *(no structural counterpart in this slice)* |
| `src/commands/doctor.ts` | *(no structural counterpart in this slice)* |
| `src/commands/env-lab.ts` | *(no structural counterpart in this slice)* |
| `src/commands/env.ts` | *(no structural counterpart in this slice)* |
| `src/commands/heartbeat-run.ts` | *(no structural counterpart in this slice)* |
| `src/commands/install.ts` | *(no structural counterpart in this slice)* |
| `src/commands/onboard.ts` | *(no structural counterpart in this slice)* |
| `src/commands/pipelines.ts` | *(no structural counterpart in this slice)* |
| `src/commands/routines.ts` | *(no structural counterpart in this slice)* |
| `src/commands/run.ts` | *(no structural counterpart in this slice)* |
| `src/commands/service.ts` | *(no structural counterpart in this slice)* |
| `src/commands/uninstall.ts` | *(no structural counterpart in this slice)* |
| `src/commands/update.ts` | *(no structural counterpart in this slice)* |
| `src/commands/worktree-lib.ts` | *(no structural counterpart in this slice)* |
| `src/commands/worktree-merge-history-lib.ts` | *(no structural counterpart in this slice)* |
| `src/commands/worktree.ts` | *(no structural counterpart in this slice)* |
| `src/config/data-dir.ts` | *(no structural counterpart in this slice)* |
| `src/config/env.ts` | *(no structural counterpart in this slice)* |
| `src/config/home.ts` | *(no structural counterpart in this slice)* |
| `src/config/hostnames.ts` | *(no structural counterpart in this slice)* |
| `src/config/schema.ts` | *(no structural counterpart in this slice)* |
| `src/config/secrets-key.ts` | *(no structural counterpart in this slice)* |
| `src/config/server-bind.ts` | *(no structural counterpart in this slice)* |
| `src/config/store.ts` | *(no structural counterpart in this slice)* |
| `src/index.ts` | *(no structural counterpart in this slice)* |
| `src/install-store.ts` | *(no structural counterpart in this slice)* |
| `src/onboard-service.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/database.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/llm.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/logging.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/secrets.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/server.ts` | *(no structural counterpart in this slice)* |
| `src/prompts/storage.ts` | *(no structural counterpart in this slice)* |
| `src/services/service-manager.ts` | *(no structural counterpart in this slice)* |
| `src/telemetry.ts` | *(no structural counterpart in this slice)* |
| `src/update-notice.ts` | *(no structural counterpart in this slice)* |
| `src/utils/banner.ts` | *(no structural counterpart in this slice)* |
| `src/utils/health-url.ts` | *(no structural counterpart in this slice)* |
| `src/utils/net.ts` | *(no structural counterpart in this slice)* |
| `src/utils/path-resolver.ts` | *(no structural counterpart in this slice)* |
| `src/version.ts` | *(no structural counterpart in this slice)* |
| `vitest.config.ts` | *(no structural counterpart in this slice)* |

## UI

按源文件名和目录做初筛；前端页面可达性、权限、状态和交互需在 UI 阶段逐页验收。

- Paperclip: **506**
- Parrot: **369**

| Paperclip evidence | Parrot evidence |
|---|---|
| `api/agents.ts` | `api/agents.ts` |
| `api/builtInAgents.ts` | `api/builtInAgents.ts` |
| `api/inbox-agent-policy.ts` | `api/instanceSettings.ts` |
| `api/instanceSettings.ts` | `api/issues.test.ts` |
| `api/issues.test.ts` | `api/issues.ts` |
| `api/issues.ts` | `api/sidebarBadges.ts` |
| `api/sidebarBadges.ts` | `api/sidebarPreferences.ts` |
| `api/sidebarPreferences.ts` | `components/ActiveAgentsPanel.test.tsx` |
| `components/ActiveAgentsPanel.test.tsx` | `components/ActiveAgentsPanel.tsx` |
| `components/ActiveAgentsPanel.tsx` | `components/AgentActionButtons.test.tsx` |
| `components/AgentActionButtons.test.tsx` | `components/AgentActionButtons.tsx` |
| `components/AgentActionButtons.tsx` | `components/AgentBubbleActionRow.tsx` |
| `components/AgentBubbleActionRow.tsx` | `components/AgentCapsule.test.tsx` |
| `components/AgentCapsule.test.tsx` | `components/AgentCapsule.tsx` |
| `components/AgentCapsule.tsx` | `components/AgentConfigForm.render.test.tsx` |
| `components/AgentConfigForm.render.test.tsx` | `components/AgentConfigForm.test.ts` |
| `components/AgentConfigForm.test.ts` | `components/AgentConfigForm.tsx` |
| `components/AgentConfigForm.tsx` | `components/AgentIconPicker.tsx` |
| `components/AgentIconPicker.tsx` | `components/AgentProperties.tsx` |
| `components/AgentMultiSelect.test.tsx` | `components/BootstrapPendingPage.tsx` |
| `components/AgentMultiSelect.tsx` | `components/BudgetSidebarMarker.tsx` |
| `components/AgentProperties.tsx` | `components/BuiltInAgentBadges.tsx` |
| `components/AgentSecretAccessEditor.test.tsx` | `components/BuiltInAgentGate.test.tsx` |
| `components/AgentSecretAccessEditor.tsx` | `components/BuiltInAgentGate.tsx` |
| `components/AppConnectionSidebar.test.tsx` | `components/CompanySettingsSidebar.test.tsx` |
| `components/AppConnectionSidebar.tsx` | `components/CompanySettingsSidebar.tsx` |
| `components/AppsSidebar.test.tsx` | `components/ConfigureBuiltInAgentModal.test.tsx` |
| `components/AppsSidebar.tsx` | `components/ConfigureBuiltInAgentModal.tsx` |
| `components/BootstrapPendingPage.tsx` | `components/InstanceSidebar.test.tsx` |
| `components/BudgetSidebarMarker.tsx` | `components/InstanceSidebar.tsx` |
| `components/BuiltInAgentBadges.tsx` | `components/IssueAssignedBacklogNotice.test.tsx` |
| `components/BuiltInAgentGate.test.tsx` | `components/IssueAssignedBacklogNotice.tsx` |
| `components/BuiltInAgentGate.tsx` | `components/IssueAttachmentsSection.test.tsx` |
| `components/CompanySettingsSidebar.test.tsx` | `components/IssueAttachmentsSection.tsx` |
| `components/CompanySettingsSidebar.tsx` | `components/IssueBlockedNotice.test.tsx` |
| `components/ConfigureBuiltInAgentModal.test.tsx` | `components/IssueBlockedNotice.tsx` |
| `components/ConfigureBuiltInAgentModal.tsx` | `components/IssueCasesPanel.test.tsx` |
| `components/InboxAgentPolicyControl.test.tsx` | `components/IssueCasesPanel.tsx` |
| `components/InboxAgentPolicyControl.tsx` | `components/IssueChatComposerHandoffPreview.test.ts` |
| `components/InstanceSidebar.test.tsx` | `components/IssueChatThread.test.tsx` |
| `components/InstanceSidebar.tsx` | `components/IssueChatThread.tsx` |
| `components/IssueAssignedBacklogNotice.test.tsx` | `components/IssueChatThreadSystemNotice.test.tsx` |
| `components/IssueAssignedBacklogNotice.tsx` | `components/IssueColumns.test.tsx` |
| `components/IssueAttachmentsSection.test.tsx` | `components/IssueColumns.tsx` |
| `components/IssueAttachmentsSection.tsx` | `components/IssueContinuationHandoff.test.tsx` |
| `components/IssueBlockedNotice.test.tsx` | `components/IssueContinuationHandoff.tsx` |
| `components/IssueBlockedNotice.tsx` | `components/IssueDocumentAnnotations.test.tsx` |
| `components/IssueCasesPanel.test.tsx` | `components/IssueDocumentAnnotations.tsx` |
| `components/IssueCasesPanel.tsx` | `components/IssueDocumentsSection.test.tsx` |
| `components/IssueChatComposerHandoffPreview.test.ts` | `components/IssueDocumentsSection.tsx` |
| `components/IssueChatThread.test.tsx` | `components/IssueFiltersPopover.test.tsx` |
| `components/IssueChatThread.tsx` | `components/IssueFiltersPopover.tsx` |
| `components/IssueChatThreadSystemNotice.test.tsx` | `components/IssueGroupHeader.tsx` |
| `components/IssueColumns.test.tsx` | `components/IssueLinkQuicklook.test.tsx` |
| `components/IssueColumns.tsx` | `components/IssueLinkQuicklook.tsx` |
| `components/IssueContinuationHandoff.test.tsx` | `components/IssueMonitorActivityCard.test.tsx` |
| `components/IssueContinuationHandoff.tsx` | `components/IssueMonitorActivityCard.tsx` |
| `components/IssueDocumentAnnotations.test.tsx` | `components/IssuePlanDecompositionsSection.tsx` |
| `components/IssueDocumentAnnotations.tsx` | `components/IssueProperties.test.tsx` |
| `components/IssueDocumentsSection.test.tsx` | `components/IssueProperties.tsx` |
| `components/IssueDocumentsSection.tsx` | `components/IssueRecoveryActionCard.test.tsx` |
| `components/IssueFieldChangeReceipt.test.tsx` | `components/IssueRecoveryActionCard.tsx` |
| `components/IssueFieldChangeReceipt.tsx` | `components/IssueReferenceActivitySummary.tsx` |
| `components/IssueFiltersPopover.test.tsx` | `components/IssueReferencePill.tsx` |
| `components/IssueFiltersPopover.tsx` | `components/IssueRelatedWorkPanel.test.tsx` |
| `components/IssueGroupHeader.tsx` | `components/IssueRelatedWorkPanel.tsx` |
| `components/IssueLinkQuicklook.test.tsx` | `components/IssueRow.test.tsx` |
| `components/IssueLinkQuicklook.tsx` | `components/IssueRow.tsx` |
| `components/IssueMonitorBanner.test.tsx` | `components/IssueRunLedger.test.tsx` |
| `components/IssueMonitorBanner.tsx` | `components/IssueRunLedger.tsx` |
| `components/IssuePlanDecompositionsSection.tsx` | `components/IssueScheduledRetryCard.test.tsx` |
| `components/IssueProperties.test.tsx` | `components/IssueScheduledRetryCard.tsx` |
| `components/IssueProperties.tsx` | `components/IssueSiblingNavigation.test.tsx` |
| `components/IssueRecoveryActionCard.test.tsx` | `components/IssueSiblingNavigation.tsx` |
| `components/IssueRecoveryActionCard.tsx` | `components/IssueThreadInteractionCard.test.tsx` |
| `components/IssueReferenceActivitySummary.tsx` | `components/IssueThreadInteractionCard.tsx` |
| `components/IssueReferencePill.tsx` | `components/IssueWorkspaceCard.test.tsx` |
| `components/IssueRelatedWorkPanel.test.tsx` | `components/IssueWorkspaceCard.tsx` |
| `components/IssueRelatedWorkPanel.tsx` | `components/IssuesList.test.tsx` |
| `components/IssueRow.test.tsx` | `components/IssuesList.tsx` |
| `components/IssueRow.tsx` | `components/IssuesQuicklook.tsx` |
| `components/IssueRunLedger.test.tsx` | `components/NewAgentDialog.test.tsx` |
| `components/IssueRunLedger.tsx` | `components/NewAgentDialog.tsx` |
| `components/IssueScheduledRetryCard.test.tsx` | `components/NewIssueDialog.test.tsx` |
| `components/IssueScheduledRetryCard.tsx` | `components/NewIssueDialog.tsx` |
| `components/IssueSiblingNavigation.test.tsx` | `components/PageSkeleton.tsx` |
| `components/IssueSiblingNavigation.tsx` | `components/PageTabBar.tsx` |
| `components/IssueThreadInteractionCard.test.tsx` | `components/RequestCollapsedSidebar.test.tsx` |
| `components/IssueThreadInteractionCard.tsx` | `components/RequestCollapsedSidebar.tsx` |
| `components/IssueWorkspaceCard.test.tsx` | `components/RouteErrorBoundary.test.tsx` |
| `components/IssueWorkspaceCard.tsx` | `components/RouteErrorBoundary.tsx` |
| `components/IssueWriteDenialNotice.test.tsx` | `components/RoutineSubSidebar.test.tsx` |
| `components/IssueWriteDenialNotice.tsx` | `components/RoutineSubSidebar.tsx` |
| `components/IssuesList.test.tsx` | `components/SecondarySidebar.tsx` |
| `components/IssuesList.tsx` | `components/Sidebar.test.tsx` |
| `components/IssuesQuicklook.tsx` | `components/Sidebar.tsx` |
| `components/NewAgentDialog.test.tsx` | `components/SidebarAccountMenu.test.tsx` |
| `components/NewAgentDialog.tsx` | `components/SidebarAccountMenu.tsx` |
| `components/NewIssueDialog.test.tsx` | `components/SidebarAgents.test.tsx` |
| `components/NewIssueDialog.tsx` | `components/SidebarAgents.tsx` |
| `components/PageSkeleton.tsx` | `components/SidebarCompanyMenu.test.tsx` |
| `components/PageTabBar.tsx` | `components/SidebarCompanyMenu.tsx` |
| `components/RequestCollapsedSidebar.test.tsx` | `components/SidebarNavItem.test.tsx` |
| `components/RequestCollapsedSidebar.tsx` | `components/SidebarNavItem.tsx` |
| `components/RouteErrorBoundary.test.tsx` | `components/SidebarProjects.test.tsx` |
| `components/RouteErrorBoundary.tsx` | `components/SidebarProjects.tsx` |
| `components/RoutineSubSidebar.test.tsx` | `components/SidebarSection.test.tsx` |
| `components/RoutineSubSidebar.tsx` | `components/SidebarSection.tsx` |
| `components/SecondarySidebar.tsx` | `components/SidebarServerInfo.test.tsx` |
| `components/Sidebar.test.tsx` | `components/SidebarServerInfo.tsx` |
| `components/Sidebar.tsx` | `components/SidebarShell.test.tsx` |
| `components/SidebarAccountMenu.test.tsx` | `components/SidebarShell.tsx` |
| `components/SidebarAccountMenu.tsx` | `components/SidebarStarredProjects.test.tsx` |
| `components/SidebarAgents.test.tsx` | `components/SidebarStarredProjects.tsx` |
| `components/SidebarAgents.tsx` | `components/access/CompanySettingsNav.test.tsx` |
| `components/SidebarCompanyMenu.test.tsx` | `components/access/CompanySettingsNav.tsx` |
| `components/SidebarCompanyMenu.tsx` | `components/agent-config-defaults.ts` |
| `components/SidebarNavItem.test.tsx` | `components/agent-config-primitives.tsx` |
| `components/SidebarNavItem.tsx` | `components/issue-output/IssueOutputSection.test.tsx` |
| `components/SidebarProjects.test.tsx` | `components/issue-output/IssueOutputSection.tsx` |
| `components/SidebarProjects.tsx` | `components/issue-output/OutputFileTile.tsx` |
| `components/SidebarSection.test.tsx` | `components/issue-output/OutputPrimaryCard.tsx` |
| `components/SidebarSection.tsx` | `components/issue-output/OutputRow.tsx` |
| `components/SidebarServerInfo.test.tsx` | `components/issue-output/OutputVideoPlayer.tsx` |
| `components/SidebarServerInfo.tsx` | `components/issue-properties/IssueProperties.tsx` |
| `components/SidebarShell.test.tsx` | `components/issue-properties/external-object-rows.tsx` |
| `components/SidebarShell.tsx` | `components/issue-properties/helpers.ts` |
| `components/SidebarStarredProjects.test.tsx` | `components/issue-properties/index.ts` |
| `components/SidebarStarredProjects.tsx` | `components/issue-properties/primitives.tsx` |
| `components/access/CompanySettingsNav.test.tsx` | `components/issue-properties/property-picker.tsx` |
| `components/access/CompanySettingsNav.tsx` | `components/issue-properties/relation-controls.tsx` |
| `components/agent-config-defaults.ts` | `components/skill-studio/AgentsUsingSkillDialog.test.tsx` |
| `components/agent-config-primitives.tsx` | `components/skill-studio/AgentsUsingSkillDialog.tsx` |
| `components/issue-output/IssueOutputSection.test.tsx` | `context/GeneralSettingsContext.tsx` |
| `components/issue-output/IssueOutputSection.tsx` | `context/SidebarContext.test.tsx` |
| `components/issue-output/OutputFileTile.tsx` | `context/SidebarContext.tsx` |
| `components/issue-output/OutputPrimaryCard.tsx` | `fixtures/issueChatLongThreadFixture.test.ts` |
| `components/issue-output/OutputRow.tsx` | `fixtures/issueChatLongThreadFixture.ts` |
| `components/issue-output/OutputVideoPlayer.tsx` | `fixtures/issueChatUxFixtures.ts` |
| `components/issue-properties/IssueProperties.tsx` | `fixtures/issueThreadInteractionFixtures.ts` |
| `components/issue-properties/IssuePropertiesArtifactsTab.tsx` | `hooks/useAgentOrder.ts` |
| `components/issue-properties/IssuePropertiesDocumentAnnotations.test.tsx` | `hooks/useCompanyPageMemory.test.ts` |
| `components/issue-properties/IssuePropertiesPlansTab.tsx` | `hooks/useCompanyPageMemory.ts` |
| `components/issue-properties/external-object-rows.tsx` | `hooks/useIssueExternalObjects.ts` |
| `components/issue-properties/helpers.ts` | `hooks/usePaperclipIssueRuntime.test.tsx` |
| `components/issue-properties/index.ts` | `hooks/usePaperclipIssueRuntime.ts` |
| `components/issue-properties/primitives.tsx` | `lib/agent-config-patch.test.ts` |
| `components/issue-properties/property-picker.tsx` | `lib/agent-config-patch.ts` |
| `components/issue-properties/relation-controls.tsx` | `lib/agent-icons.ts` |
| `components/skill-studio/AgentsUsingSkillDialog.test.tsx` | `lib/agent-onboarding-prompt.test.ts` |
| `components/skill-studio/AgentsUsingSkillDialog.tsx` | `lib/agent-onboarding-prompt.ts` |
| `context/GeneralSettingsContext.tsx` | `lib/agent-order.test.ts` |
| `context/SidebarContext.test.tsx` | `lib/agent-order.ts` |
| `context/SidebarContext.tsx` | `lib/agent-skills-state.test.ts` |
| `fixtures/issueChatLongThreadFixture.test.ts` | `lib/agent-skills-state.ts` |
| `fixtures/issueChatLongThreadFixture.ts` | `lib/built-in-agent-toast.ts` |
| `fixtures/issueChatUxFixtures.ts` | `lib/company-page-memory.ts` |
| `fixtures/issueThreadInteractionFixtures.ts` | `lib/company-portability-sidebar.test.ts` |
| `hooks/useAgentOrder.ts` | `lib/company-portability-sidebar.ts` |
| `hooks/useCompanyPageMemory.test.ts` | `lib/company-routes.test.ts` |
| `hooks/useCompanyPageMemory.ts` | `lib/company-routes.ts` |
| `hooks/useIssueDocuments.ts` | `lib/company-skill-routes.test.ts` |
| `hooks/useIssueExternalObjects.ts` | `lib/company-skill-routes.ts` |
| `hooks/useIssuePlanDocument.ts` | `lib/duplicate-agent-payload.test.ts` |
| `hooks/usePaperclipIssueRuntime.test.tsx` | `lib/duplicate-agent-payload.ts` |
| `hooks/usePaperclipIssueRuntime.ts` | `lib/instance-settings.test.ts` |
| `lib/agent-config-patch.test.ts` | `lib/instance-settings.ts` |
| `lib/agent-config-patch.ts` | `lib/issue-assignee-overrides.test.ts` |
| `lib/agent-icons.ts` | `lib/issue-assignee-overrides.ts` |
| `lib/agent-onboarding-prompt.test.ts` | `lib/issue-attachments.ts` |
| `lib/agent-onboarding-prompt.ts` | `lib/issue-blockers.ts` |
| `lib/agent-order.test.ts` | `lib/issue-chat-messages.test.ts` |
| `lib/agent-order.ts` | `lib/issue-chat-messages.ts` |
| `lib/agent-skills-state.test.ts` | `lib/issue-chat-scroll.test.ts` |
| `lib/agent-skills-state.ts` | `lib/issue-chat-scroll.ts` |
| `lib/built-in-agent-toast.ts` | `lib/issue-detail-subissues.test.ts` |
| `lib/company-page-memory.ts` | `lib/issue-detail-subissues.ts` |
| `lib/company-portability-sidebar.test.ts` | `lib/issue-execution-policy.test.ts` |
| `lib/company-portability-sidebar.ts` | `lib/issue-execution-policy.ts` |
| `lib/company-routes.test.ts` | `lib/issue-filters.test.ts` |
| `lib/company-routes.ts` | `lib/issue-filters.ts` |
| `lib/company-skill-routes.test.ts` | `lib/issue-monitor.ts` |
| `lib/company-skill-routes.ts` | `lib/issue-output.test.ts` |
| `lib/duplicate-agent-payload.test.ts` | `lib/issue-output.ts` |
| `lib/duplicate-agent-payload.ts` | `lib/issue-properties-panel-key.test.ts` |
| `lib/instance-settings.test.ts` | `lib/issue-properties-panel-key.ts` |
| `lib/instance-settings.ts` | `lib/issue-reference.test.ts` |
| `lib/issue-artifacts.test.ts` | `lib/issue-reference.ts` |
| `lib/issue-artifacts.ts` | `lib/issue-thread-interactions.test.ts` |
| `lib/issue-assignee-overrides.test.ts` | `lib/issue-thread-interactions.ts` |
| `lib/issue-assignee-overrides.ts` | `lib/issue-timeline-events.test.ts` |
| `lib/issue-attachments.ts` | `lib/issue-timeline-events.ts` |
| `lib/issue-blockers.ts` | `lib/issue-tree.test.ts` |
| `lib/issue-change-receipt.test.ts` | `lib/issue-tree.ts` |
| `lib/issue-change-receipt.ts` | `lib/issueActiveRun.test.ts` |
| `lib/issue-chat-messages.test.ts` | `lib/issueActiveRun.ts` |
| `lib/issue-chat-messages.ts` | `lib/issueChatTranscriptRuns.test.ts` |
| `lib/issue-chat-scroll.test.ts` | `lib/issueChatTranscriptRuns.ts` |
| `lib/issue-chat-scroll.ts` | `lib/issueDetailBreadcrumb.test.ts` |
| `lib/issue-detail-performance.ts` | `lib/issueDetailBreadcrumb.ts` |
| `lib/issue-detail-subissues.test.ts` | `lib/issueDetailCache.test.ts` |
| `lib/issue-detail-subissues.ts` | `lib/issueDetailCache.ts` |
| `lib/issue-document-deep-link.test.ts` | `lib/issueDetailQuery.test.tsx` |
| `lib/issue-document-deep-link.ts` | `lib/legacy-agent-config.test.ts` |
| `lib/issue-execution-policy.test.ts` | `lib/legacy-agent-config.ts` |
| `lib/issue-execution-policy.ts` | `lib/liveIssueIds.test.ts` |
| `lib/issue-filters.test.ts` | `lib/liveIssueIds.ts` |
| `lib/issue-filters.ts` | `lib/new-agent-hire-payload.test.ts` |
| `lib/issue-monitor.test.tsx` | `lib/new-agent-hire-payload.ts` |
| `lib/issue-monitor.ts` | `lib/new-agent-runtime-config.test.ts` |
| `lib/issue-output.test.ts` | `lib/new-agent-runtime-config.ts` |
| `lib/issue-output.ts` | `lib/onboarding-route.test.ts` |
| `lib/issue-properties-panel-key.test.ts` | `lib/onboarding-route.ts` |
| `lib/issue-properties-panel-key.ts` | `lib/optimistic-issue-comments.test.ts` |
| `lib/issue-reference.test.ts` | `lib/optimistic-issue-comments.ts` |
| `lib/issue-reference.ts` | `lib/optimistic-issue-runs.test.ts` |
| `lib/issue-thread-interactions.test.ts` | `lib/optimistic-issue-runs.ts` |
| `lib/issue-thread-interactions.ts` | `lib/page-visibility.test.ts` |
| `lib/issue-timeline-events.test.ts` | `lib/page-visibility.ts` |
| `lib/issue-timeline-events.ts` | `lib/paperclip-shared/src/agent-eligibility.test.ts` |
| `lib/issue-tree.test.ts` | `lib/paperclip-shared/src/agent-eligibility.ts` |
| `lib/issue-tree.ts` | `lib/paperclip-shared/src/agent-url-key.ts` |
| `lib/issue-write-denial-activity.ts` | `lib/paperclip-shared/src/issue-attribution.test.ts` |
| `lib/issueActiveRun.test.ts` | `lib/paperclip-shared/src/issue-attribution.ts` |
| `lib/issueActiveRun.ts` | `lib/paperclip-shared/src/issue-references.test.ts` |
| `lib/issueChatTranscriptRuns.test.ts` | `lib/paperclip-shared/src/issue-references.ts` |
| `lib/issueChatTranscriptRuns.ts` | `lib/paperclip-shared/src/issue-thread-interactions.test.ts` |
| `lib/issueDetailBreadcrumb.test.ts` | `lib/paperclip-shared/src/types/agent.ts` |
| `lib/issueDetailBreadcrumb.ts` | `lib/paperclip-shared/src/types/issue-tree-control.ts` |
| `lib/issueDetailCache.test.ts` | `lib/paperclip-shared/src/types/issue.ts` |
| `lib/issueDetailCache.ts` | `lib/paperclip-shared/src/types/sidebar-badges.ts` |
| `lib/issueDetailQuery.test.tsx` | `lib/paperclip-shared/src/types/sidebar-preferences.ts` |
| `lib/legacy-agent-config.test.ts` | `lib/paperclip-shared/src/validators/agent.ts` |
| `lib/legacy-agent-config.ts` | `lib/paperclip-shared/src/validators/issue-tree-control.ts` |
| `lib/liveIssueIds.test.ts` | `lib/paperclip-shared/src/validators/issue.test.ts` |
| `lib/liveIssueIds.ts` | `lib/paperclip-shared/src/validators/issue.ts` |
| `lib/new-agent-hire-payload.test.ts` | `lib/paperclip-shared/src/validators/sidebar-preferences.ts` |
| `lib/new-agent-hire-payload.ts` | `lib/router.tsx` |
| `lib/new-agent-runtime-config.test.ts` | `lib/subIssueDefaults.test.ts` |
| `lib/new-agent-runtime-config.ts` | `lib/subIssueDefaults.ts` |
| `lib/onboarding-route.test.ts` | `pages/Activity.tsx` |
| `lib/onboarding-route.ts` | `pages/AdapterManager.tsx` |
| `lib/optimistic-issue-comments.test.ts` | `pages/AgentDetail.instructions.test.tsx` |
| `lib/optimistic-issue-comments.ts` | `pages/AgentDetail.progress.test.ts` |
| `lib/optimistic-issue-runs.test.ts` | `pages/AgentDetail.tsx` |
| `lib/optimistic-issue-runs.ts` | `pages/Agents.test.tsx` |
| `lib/page-visibility.test.ts` | `pages/Agents.tsx` |
| `lib/page-visibility.ts` | `pages/ApprovalDetail.tsx` |
| `lib/prefetchIssueComments.test.ts` | `pages/Approvals.tsx` |
| `lib/router.tsx` | `pages/Artifacts.test.tsx` |
| `lib/subIssueDefaults.test.ts` | `pages/Artifacts.tsx` |
| `lib/subIssueDefaults.ts` | `pages/Auth.test.tsx` |
| `pages/AdapterManager.tsx` | `pages/Auth.tsx` |
| `pages/AgentDetail.instructions.test.tsx` | `pages/BoardChat.test.tsx` |
| `pages/AgentDetail.liveRun.test.ts` | `pages/BoardChat.tsx` |
| `pages/AgentDetail.progress.test.ts` | `pages/BoardClaim.tsx` |
| `pages/AgentDetail.tsx` | `pages/BootstrapSetupUxLab.tsx` |
| `pages/AgentToolsTab.test.tsx` | `pages/CaseDetail.test.tsx` |
| `pages/AgentToolsTab.tsx` | `pages/CaseDetail.tsx` |
| `pages/Agents.test.tsx` | `pages/Cases.test.tsx` |
| `pages/Agents.tsx` | `pages/Cases.tsx` |
| `pages/ApprovalDetail.tsx` | `pages/CliAuth.tsx` |
| `pages/Approvals.tsx` | `pages/CloudUpstream.test.tsx` |
| `pages/Artifacts.test.tsx` | `pages/CloudUpstream.tsx` |
| `pages/Artifacts.tsx` | `pages/CloudUpstreamUxLab.tsx` |
| `pages/Auth.test.tsx` | `pages/Companies.tsx` |
| `pages/Auth.tsx` | `pages/CompanyAccess.test.tsx` |
| `pages/BoardChat.test.tsx` | `pages/CompanyAccess.tsx` |
| `pages/BoardChat.tsx` | `pages/CompanyEnvironments.test.tsx` |
| `pages/BoardClaim.tsx` | `pages/CompanyEnvironments.tsx` |
| `pages/BootstrapSetupUxLab.tsx` | `pages/CompanyExport.tsx` |
| `pages/CaseDetail.test.tsx` | `pages/CompanyImport.tsx` |
| `pages/CaseDetail.tsx` | `pages/CompanyInvites.test.tsx` |
| `pages/Cases.test.tsx` | `pages/CompanyInvites.tsx` |
| `pages/Cases.tsx` | `pages/CompanySettings.test.tsx` |
| `pages/CliAuth.tsx` | `pages/CompanySettings.tsx` |
| `pages/Companies.test.tsx` | `pages/CompanySettingsPluginPage.test.tsx` |
| `pages/Companies.tsx` | `pages/CompanySettingsPluginPage.tsx` |
| `pages/CompanyAccess.test.tsx` | `pages/CompanySkills.test.tsx` |
| `pages/CompanyAccess.tsx` | `pages/CompanySkills.tsx` |
| `pages/CompanyEnvironments.test.tsx` | `pages/Costs.tsx` |
| `pages/CompanyEnvironments.tsx` | `pages/Dashboard.tsx` |
| `pages/CompanyExport.test.tsx` | `pages/DashboardLive.tsx` |
| `pages/CompanyExport.tsx` | `pages/DesignGuide.tsx` |
| `pages/CompanyImport.test.tsx` | `pages/ExecutionWorkspaceDetail.test.tsx` |
| `pages/CompanyImport.tsx` | `pages/ExecutionWorkspaceDetail.tsx` |
| `pages/CompanyInvites.test.tsx` | `pages/GoalDetail.test.tsx` |
| `pages/CompanyInvites.tsx` | `pages/GoalDetail.tsx` |
| `pages/CompanySettings.test.tsx` | `pages/Goals.tsx` |
| `pages/CompanySettings.tsx` | `pages/Inbox.test.tsx` |
| `pages/CompanySettingsPluginPage.test.tsx` | `pages/Inbox.tsx` |
| `pages/CompanySettingsPluginPage.tsx` | `pages/InstanceAccess.tsx` |
| `pages/CompanySkills.test.tsx` | `pages/InstanceExperimentalSettings.test.tsx` |
| `pages/CompanySkills.tsx` | `pages/InstanceExperimentalSettings.tsx` |
| `pages/Costs.tsx` | `pages/InstanceGeneralSettings.tsx` |
| `pages/CrossIssueCollaborationUxLab.tsx` | `pages/InstanceSettings.tsx` |
| `pages/Dashboard.tsx` | `pages/InviteLanding.test.tsx` |
| `pages/DashboardLive.tsx` | `pages/InviteLanding.tsx` |
| `pages/DecisionQueuePage.tsx` | `pages/InviteUxLab.test.tsx` |
| `pages/DesignGuide.tsx` | `pages/InviteUxLab.tsx` |
| `pages/ExecutionWorkspaceDetail.provision-status.test.ts` | `pages/IssueChatLongThreadPerf.tsx` |
| `pages/ExecutionWorkspaceDetail.service-ports.test.ts` | `pages/IssueChatUxLab.tsx` |
| `pages/ExecutionWorkspaceDetail.test.tsx` | `pages/IssueDetail.test.tsx` |
| `pages/ExecutionWorkspaceDetail.tsx` | `pages/IssueDetail.tsx` |
| `pages/GoalDetail.test.tsx` | `pages/Issues.test.tsx` |
| `pages/GoalDetail.tsx` | `pages/Issues.tsx` |
| `pages/Goals.tsx` | `pages/JoinRequestQueue.tsx` |
| `pages/Inbox.test.tsx` | `pages/MyIssues.tsx` |
| `pages/Inbox.tsx` | `pages/NewAgent.tsx` |
| `pages/InstanceAccess.tsx` | `pages/NotFound.tsx` |
| `pages/InstanceExperimentalSettings.test.tsx` | `pages/Org.tsx` |
| `pages/InstanceExperimentalSettings.tsx` | `pages/OrgChart.test.tsx` |
| `pages/InstanceGeneralSettings.test.tsx` | `pages/OrgChart.tsx` |
| `pages/InstanceGeneralSettings.tsx` | `pages/PipelineSettings.test.ts` |
| `pages/InstanceSettings.tsx` | `pages/PipelineSettings.tsx` |
| `pages/InviteLanding.test.tsx` | `pages/Pipelines.test.tsx` |
| `pages/InviteLanding.tsx` | `pages/Pipelines.tsx` |
| `pages/InviteUxLab.test.tsx` | `pages/PluginManager.tsx` |
| `pages/InviteUxLab.tsx` | `pages/PluginPage.test.tsx` |
| `pages/IssueChatLongThreadPerf.tsx` | `pages/PluginPage.tsx` |
| `pages/IssueChatUxLab.tsx` | `pages/PluginSettings.test.tsx` |
| `pages/IssueDetail.test.tsx` | `pages/PluginSettings.tsx` |
| `pages/IssueDetail.tsx` | `pages/ProfileSettings.test.tsx` |
| `pages/Issues.test.tsx` | `pages/ProfileSettings.tsx` |
| `pages/Issues.tsx` | `pages/ProjectDetail.test.tsx` |
| `pages/JoinRequestQueue.tsx` | `pages/ProjectDetail.tsx` |
| `pages/MyIssues.tsx` | `pages/ProjectWorkspaceDetail.test.tsx` |
| `pages/NewAgent.test.tsx` | `pages/ProjectWorkspaceDetail.tsx` |
| `pages/NewAgent.tsx` | `pages/Projects.test.tsx` |
| `pages/NotFound.tsx` | `pages/Projects.tsx` |
| `pages/Org.tsx` | `pages/ResponsibleUserDenialUxLab.tsx` |
| `pages/OrgChart.test.tsx` | `pages/RoutineDetail.tsx` |
| `pages/OrgChart.tsx` | `pages/Routines.test.tsx` |
| `pages/PipelineSettings.test.ts` | `pages/Routines.tsx` |
| `pages/PipelineSettings.tsx` | `pages/RunTranscriptUxLab.tsx` |
| `pages/Pipelines.test.tsx` | `pages/Search.test.tsx` |
| `pages/Pipelines.tsx` | `pages/Search.tsx` |
| `pages/PluginManager.tsx` | `pages/Secrets.render.test.tsx` |
| `pages/PluginPage.test.tsx` | `pages/Secrets.test.ts` |
| `pages/PluginPage.tsx` | `pages/Secrets.tsx` |
| `pages/PluginSettings.test.tsx` | `pages/SkillStudio.test.tsx` |
| `pages/PluginSettings.tsx` | `pages/SkillStudio.tsx` |
| `pages/ProfileSettings.test.tsx` | `pages/SystemNoticeUxLab.tsx` |
| `pages/ProfileSettings.tsx` | `pages/TeamCard.test.tsx` |
| `pages/ProjectDetail.test.tsx` | `pages/TeamCatalog.fixtures.ts` |
| `pages/ProjectDetail.tsx` | `pages/TeamCatalog.test.tsx` |
| `pages/ProjectWorkspaceDetail.test.tsx` | `pages/TeamCatalog.tsx` |
| `pages/ProjectWorkspaceDetail.tsx` | `pages/Timeline.test.tsx` |
| `pages/Projects.test.tsx` | `pages/Timeline.tsx` |
| `pages/Projects.tsx` | `pages/UserProfile.tsx` |
| `pages/ResponsibleUserDenialUxLab.tsx` | `pages/Workspaces.test.tsx` |
| `pages/RoutineDetail.test.tsx` | `pages/Workspaces.tsx` |
| `pages/RoutineDetail.tsx` | `pages/agent-skills/AgentSkillRow.tsx` |
| `pages/Routines.test.tsx` | `pages/agent-skills/AgentSkillsTab.tsx` |
| `pages/Routines.tsx` | `pages/agent-skills/agent-skill-filter.test.ts` |
| `pages/RunTranscriptUxLab.tsx` | `pages/agent-skills/agent-skill-filter.ts` |
| `pages/Search.test.tsx` | `pages/agent-skills/agent-skill-source.test.ts` |
| `pages/Search.tsx` | `pages/agent-skills/agent-skill-source.ts` |
| `pages/Secrets.render.test.tsx` | `pages/secrets/ImportFromVaultDialog.test.tsx` |
| `pages/Secrets.test.ts` | `pages/secrets/ImportFromVaultDialog.tsx` |
| `pages/Secrets.tsx` | `pages/secrets/MissingUserSecretsBanner.test.tsx` |
| `pages/SkillStudio.test.tsx` | `pages/secrets/MissingUserSecretsBanner.tsx` |
| `pages/SkillStudio.tsx` | `pages/secrets/MyUserSecretsTab.tsx` |
| `pages/StatusCards/ArchivedStatusCardRow.tsx` | `pages/secrets/SetMyUserSecretDialog.tsx` |
| `pages/StatusCards/CreateStatusCardDialog.tsx` | `pages/secrets/UserSecretDefinitionsTab.tsx` |
| `pages/StatusCards/StatusCardDetailDrawer.tsx` | `pages/secrets/my-value-state.ts` |
| `pages/StatusCards/StatusCardSettingsForm.test.tsx` | `pages/secrets/user-secret-presentation.test.ts` |
| `pages/StatusCards/StatusCardSettingsForm.tsx` | `pages/secrets/user-secret-presentation.tsx` |
| `pages/StatusCards/StatusCardTile.test.tsx` | `pages/useInstallTeamCatalogEntry.test.tsx` |
| `pages/StatusCards/StatusCardTile.tsx` | *(no structural counterpart in this slice)* |
| `pages/StatusCards/SummarizerAgentSelect.tsx` | *(no structural counterpart in this slice)* |
| `pages/StatusCards/format.test.ts` | *(no structural counterpart in this slice)* |
| `pages/StatusCards/format.ts` | *(no structural counterpart in this slice)* |
| `pages/StatusCards/index.tsx` | *(no structural counterpart in this slice)* |
| `pages/StatusCards/types.ts` | *(no structural counterpart in this slice)* |
| `pages/SystemNoticeUxLab.tsx` | *(no structural counterpart in this slice)* |
| `pages/TaskChatLab.tsx` | *(no structural counterpart in this slice)* |
| `pages/TeamCard.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/TeamCatalog.fixtures.ts` | *(no structural counterpart in this slice)* |
| `pages/TeamCatalog.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/TeamCatalog.tsx` | *(no structural counterpart in this slice)* |
| `pages/Timeline.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/Timeline.tsx` | *(no structural counterpart in this slice)* |
| `pages/UserProfile.tsx` | *(no structural counterpart in this slice)* |
| `pages/WhatNeedsMe.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/WhatNeedsMe.tsx` | *(no structural counterpart in this slice)* |
| `pages/Workspaces.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/Workspaces.tsx` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/AgentSkillReleasePicker.test.ts` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/AgentSkillReleasePicker.tsx` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/AgentSkillRow.tsx` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/AgentSkillsTab.test.ts` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/AgentSkillsTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/agent-skill-filter.test.ts` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/agent-skill-filter.ts` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/agent-skill-source.test.ts` | *(no structural counterpart in this slice)* |
| `pages/agent-skills/agent-skill-source.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/AppDetail.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppDetail.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppLogo.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppNotConnected.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppNotConnected.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppsConnect.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppsConnect.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/AppsReview.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/Browse.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/Browse.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/Connections.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/Connections.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/ReviewQueueCard.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/ReviewQueueCard.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-connect-policy.test.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/app-connect-policy.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/app-definition-display.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/ActivityPanel.render.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/ActivityPanel.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/ActivityPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/AdvancedPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/PermissionsPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/ReviewPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/SetupPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/TestPanel.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/TestPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/app-detail/types.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/app-tabs.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/AppsSubNav.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/ConnectClientDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/GatewayDetail.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/GatewaysList.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/NewGatewayDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/gateway-helpers.test.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/gateway-helpers.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/gateway-tabs.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/AppsToolsPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/GatewayActivityPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/GatewayAdvancedPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/OverviewPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/TokensPanel.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/gateways/panels/TokensPanel.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/google-sheets.ts` | *(no structural counterpart in this slice)* |
| `pages/apps/store-cards.tsx` | *(no structural counterpart in this slice)* |
| `pages/apps/useReviewCount.ts` | *(no structural counterpart in this slice)* |
| `pages/audit/AuditFeed.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/audit/AuditFeed.tsx` | *(no structural counterpart in this slice)* |
| `pages/audit/CompanyActivity.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/ImportFromVaultDialog.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/ImportFromVaultDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/MissingUserSecretsBanner.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/MissingUserSecretsBanner.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/MyUserSecretsTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/ProposalsTab.render.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/ProposalsTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/SecretPathName.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/SetMyUserSecretDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/UserSecretDefinitionsTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/my-value-state.ts` | *(no structural counterpart in this slice)* |
| `pages/secrets/proposal-review.tsx` | *(no structural counterpart in this slice)* |
| `pages/secrets/secret-path.test.ts` | *(no structural counterpart in this slice)* |
| `pages/secrets/secret-path.ts` | *(no structural counterpart in this slice)* |
| `pages/secrets/user-secret-presentation.test.ts` | *(no structural counterpart in this slice)* |
| `pages/secrets/user-secret-presentation.tsx` | *(no structural counterpart in this slice)* |
| `pages/skills/ImportSkillsFromProjectDialog.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/skills/ImportSkillsFromProjectDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/AdvancedToolsRoute.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/AuditTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/AuditTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/GatewaysTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/GatewaysTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/PasteConfigTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/PasteConfigTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/PoliciesTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/PoliciesTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/ProfilesTab.test.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/ProfilesTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/RunYourOwnTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/RuntimeTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/RuntimeTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/SmokeLabTab.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/SmokeLabTab.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/ToolsAccess.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/ToolsAccess.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/connection-dialogs.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileActionDialog.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileDetail.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileDetail.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileDetailRoute.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileWizard.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileWizard.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfileWizardRoute.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfilesIndex.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ProfilesIndex.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/ToolsAdminGate.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/WizardToolsStep.test.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/WizardToolsStep.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/profile-model.test.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/profile-model.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/profile-summary.test.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/profile-summary.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/useProfilesData.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/wizard-draft.test.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/profiles/wizard-draft.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/shared.tsx` | *(no structural counterpart in this slice)* |
| `pages/tools/smoke-lab-matrix.test.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/smoke-lab-matrix.ts` | *(no structural counterpart in this slice)* |
| `pages/tools/tool-tabs.ts` | *(no structural counterpart in this slice)* |
| `pages/useInstallTeamCatalogEntry.test.tsx` | *(no structural counterpart in this slice)* |

## Worker / Scheduler

按 Worker、Job、Scheduler、Cron、Heartbeat 等关键词做初筛；需继续核对触发周期、幂等、恢复和并发策略。

- Paperclip: **65**
- Parrot: **20**

| Paperclip evidence | Parrot evidence |
|---|---|
| `__tests__/decision-queues-routes.test.ts` | `decision_retention_sweep_job.rs` |
| `__tests__/heartbeat-accepted-plan-workspace-refresh.test.ts` | `heartbeat_service.rs` |
| `__tests__/heartbeat-active-run-output-watchdog.test.ts` | `job_scheduler.rs` |
| `__tests__/heartbeat-agent-session-message.test.ts` | `monitor_scheduler.rs` |
| `__tests__/heartbeat-archived-company-guard.test.ts` | `plugin_job_coordinator.rs` |
| `__tests__/heartbeat-auto-checkout.test.ts` | `plugin_job_scheduler.rs` |
| `__tests__/heartbeat-comment-wake-batching.test.ts` | `plugin_worker_manager.rs` |
| `__tests__/heartbeat-context-summary.test.ts` | `recovery_action_service.rs` |
| `__tests__/heartbeat-cost-accounting.test.ts` | `recovery_observability_service.rs` |
| `__tests__/heartbeat-dependency-scheduling.test.ts` | `routine_annotation_service.rs` |
| `__tests__/heartbeat-issue-liveness-escalation.test.ts` | `routine_coordinator_service.rs` |
| `__tests__/heartbeat-issue-rewake-throttle.test.ts` | `routine_execution_service.rs` |
| `__tests__/heartbeat-ledger-billing-code.test.ts` | `routine_service.rs` |
| `__tests__/heartbeat-list.test.ts` | `routine_service_impl.rs` |
| `__tests__/heartbeat-local-environment.test.ts` | `routine_template.rs` |
| `__tests__/heartbeat-lock-release-on-reassignment.test.ts` | `routine_trigger_service.rs` |
| `__tests__/heartbeat-managed-clone-credentials.test.ts` | `routine_variable_service.rs` |
| `__tests__/heartbeat-model-profile.test.ts` | `sagas/routine_trigger_saga.rs` |
| `__tests__/heartbeat-pending-cleanup-sweep.test.ts` | `status_card_worker.rs` |
| `__tests__/heartbeat-plugin-environment.test.ts` | `summary_slot_worker.rs` |
| `__tests__/heartbeat-process-recovery.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-project-env.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-referenced-projects.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-remote-referenced-projects.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-responsible-user-invariant.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-retry-scheduling.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-run-lease-release-terminalization.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-run-log.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-run-status-payload.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-run-summary.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-run-terminalize-before-release.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-runtime-mcp-servers.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-runtime-skills.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-runtime-state.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-scheduling-suppression.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-stale-queue-invalidation.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-start-lock.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-timer-wake-session-reset-pf4.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-workspace-branch-containment.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-workspace-busy.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-workspace-finalize-branch.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-workspace-ready-comment.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-workspace-session.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-worktree-suppression.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/heartbeat-zombie-guard.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/helpers/drain-heartbeat-runs.ts` | *(no structural counterpart in this slice)* |
| `__tests__/issue-monitor-scheduler.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/plugin-worker-manager.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/task-watchdogs-scheduler.test.ts` | *(no structural counterpart in this slice)* |
| `__tests__/tool-review-queue-unsigned-request.test.ts` | *(no structural counterpart in this slice)* |
| `routes/decision-queues.ts` | *(no structural counterpart in this slice)* |
| `services/cron.ts` | *(no structural counterpart in this slice)* |
| `services/decision-queues.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat-run-runtime-status.test.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat-run-runtime-status.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat-run-summary.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat-stop-metadata.test.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat-stop-metadata.ts` | *(no structural counterpart in this slice)* |
| `services/heartbeat.ts` | *(no structural counterpart in this slice)* |
| `services/plugin-job-coordinator.ts` | *(no structural counterpart in this slice)* |
| `services/plugin-job-scheduler.ts` | *(no structural counterpart in this slice)* |
| `services/plugin-job-store.ts` | *(no structural counterpart in this slice)* |
| `services/plugin-worker-manager.ts` | *(no structural counterpart in this slice)* |
| `services/workspace-git-operation-scheduler.test.ts` | *(no structural counterpart in this slice)* |
| `services/workspace-git-operation-scheduler.ts` | *(no structural counterpart in this slice)* |

## Provider / Adapter / Sandbox / Storage / Secret

按 provider/adapter/sandbox/storage/secret 关键词做初筛；需继续核对运行时能力矩阵和安全边界。

- Paperclip: **486**
- Parrot: **69**

| Paperclip evidence | Parrot evidence |
|---|---|
| `adapter-utils/src/acpx-engine/cli.ts` | `adapters/src/adapter_trait.rs` |
| `adapter-utils/src/acpx-engine/composed-run-characterization.test.ts` | `adapters/src/claude_local_adapter.rs` |
| `adapter-utils/src/acpx-engine/constants.ts` | `adapters/src/lib.rs` |
| `adapter-utils/src/acpx-engine/execute-identity.test.ts` | `adapters/src/process_adapter.rs` |
| `adapter-utils/src/acpx-engine/execute.test.ts` | `adapters/src/registry.rs` |
| `adapter-utils/src/acpx-engine/execute.ts` | `api/src/routes/adapters.rs` |
| `adapter-utils/src/acpx-engine/index.ts` | `api/src/routes/secret_proposals.rs` |
| `adapter-utils/src/acpx-engine/remote-spawn-smoke.test.ts` | `api/src/routes/secret_provider_configs.rs` |
| `adapter-utils/src/acpx-engine/run-contracts.test.ts` | `api/src/routes/secret_remote_import.rs` |
| `adapter-utils/src/acpx-engine/run-contracts.ts` | `api/src/routes/secrets.rs` |
| `adapter-utils/src/acpx-engine/run-coordinator.test.ts` | `api/src/routes/user_secret_definitions.rs` |
| `adapter-utils/src/acpx-engine/run-coordinator.ts` | `api/src/routes/user_secrets.rs` |
| `adapter-utils/src/acpx-engine/run-fault-matrix.test.ts` | `api/src/schemas/adapter_schemas.rs` |
| `adapter-utils/src/acpx-engine/run-resource-ledger.test.ts` | `models/src/adapter.rs` |
| `adapter-utils/src/acpx-engine/run-resource-ledger.ts` | `models/src/secret_provider.rs` |
| `adapter-utils/src/acpx-engine/run-site-host.test.ts` | `models/src/secret_provider_config.rs` |
| `adapter-utils/src/acpx-engine/run-site-host.ts` | `models/src/secret_remote_import.rs` |
| `adapter-utils/src/acpx-engine/run-site-sandbox.test.ts` | `models/src/secrets.rs` |
| `adapter-utils/src/acpx-engine/run-site-sandbox.ts` | `models/src/user_secret.rs` |
| `adapter-utils/src/acpx-engine/session-codec.ts` | `models/src/user_secret_definition.rs` |
| `adapter-utils/src/acpx-engine/session-reuse-store.test.ts` | `repositories/src/secret_repository.rs` |
| `adapter-utils/src/acpx-engine/session-reuse-store.ts` | `repositories/src/user_secret_repository.rs` |
| `adapter-utils/src/acpx-engine/settlement-characterization.test.ts` | `repositories/tests/user_secret_version_test.rs` |
| `adapter-utils/src/acpx-engine/settlement-sequence.test.ts` | `server/tests/adapter_e2e_test.rs` |
| `adapter-utils/src/acpx-engine/settlement-sequence.ts` | `server/tests/provider_matrix_test.rs` |
| `adapter-utils/src/acpx-engine/spawn-smoke.test.ts` | `server/tests/secret_proposals_http_parity_test.rs` |
| `adapter-utils/src/acpx-engine/startup-characterization.test.ts` | `server/tests/secret_provider_descriptors_http_parity_test.rs` |
| `adapter-utils/src/acpx-engine/startup-timing.test.ts` | `services/src/adapter_config_normalizer.rs` |
| `adapter-utils/src/acpx-engine/startup-timing.ts` | `services/src/adapter_executor.rs` |
| `adapter-utils/src/acpx-engine/turn-characterization.test.ts` | `services/src/adapter_install_lock.rs` |
| `adapter-utils/src/acpx-engine/turn-sequence.test.ts` | `services/src/adapter_install_transaction.rs` |
| `adapter-utils/src/acpx-engine/turn-sequence.ts` | `services/src/adapter_package_loader.rs` |
| `adapter-utils/src/acpx-engine/ui.ts` | `services/src/adapter_plugin.rs` |
| `adapter-utils/src/billing.test.ts` | `services/src/adapter_plugin_store.rs` |
| `adapter-utils/src/billing.ts` | `services/src/adapter_registry.rs` |
| `adapter-utils/src/command-managed-runtime.test.ts` | `services/src/adapter_registry_state.rs` |
| `adapter-utils/src/command-managed-runtime.ts` | `services/src/adapter_runtime_secrets.rs` |
| `adapter-utils/src/command-redaction.test.ts` | `services/src/adapters/claude_local_adapter.rs` |
| `adapter-utils/src/command-redaction.ts` | `services/src/adapters/codex_local_adapter.rs` |
| `adapter-utils/src/env-bindings.test.ts` | `services/src/adapters/cursor_cloud_adapter.rs` |
| `adapter-utils/src/env-bindings.ts` | `services/src/adapters/cursor_local_adapter.rs` |
| `adapter-utils/src/exclude-patterns.ts` | `services/src/adapters/gemini_local_adapter.rs` |
| `adapter-utils/src/execution-target-sandbox.test.ts` | `services/src/adapters/grok_local_adapter.rs` |
| `adapter-utils/src/execution-target-stdin-race.test.ts` | `services/src/adapters/hermes_gateway_adapter.rs` |
| `adapter-utils/src/execution-target.test.ts` | `services/src/adapters/hermes_local_adapter.rs` |
| `adapter-utils/src/execution-target.ts` | `services/src/adapters/mod.rs` |
| `adapter-utils/src/git-workspace-sync.test.ts` | `services/src/adapters/openclaw_gateway_adapter.rs` |
| `adapter-utils/src/git-workspace-sync.ts` | `services/src/adapters/opencode_local_adapter.rs` |
| `adapter-utils/src/index.ts` | `services/src/adapters/pi_local_adapter.rs` |
| `adapter-utils/src/local-process-sandbox.test.ts` | `services/src/adapters/process_adapter.rs` |
| `adapter-utils/src/local-process-sandbox.ts` | `services/src/agent_secret_bindings_service.rs` |
| `adapter-utils/src/log-redaction.ts` | `services/src/asset_storage.rs` |
| `adapter-utils/src/mcp-isolation.integration.test.ts` | `services/src/aws_secrets_manager_provider.rs` |
| `adapter-utils/src/remote-execution-env.ts` | `services/src/builtin_adapter_types.rs` |
| `adapter-utils/src/remote-managed-runtime.test.ts` | `services/src/database_secret_service.rs` |
| `adapter-utils/src/remote-managed-runtime.ts` | `services/src/environment_driver/local_fake_sandbox_driver.rs` |
| `adapter-utils/src/runtime-progress.test.ts` | `services/src/environment_driver/sandbox_capabilities.rs` |
| `adapter-utils/src/runtime-progress.ts` | `services/src/environment_driver/sandbox_driver.rs` |
| `adapter-utils/src/sandbox-callback-bridge.test.ts` | `services/src/github_external_object_provider_service.rs` |
| `adapter-utils/src/sandbox-callback-bridge.ts` | `services/src/plugin_runtime_sandbox.rs` |
| `adapter-utils/src/sandbox-file-sync.test.ts` | `services/src/s3_storage.rs` |
| `adapter-utils/src/sandbox-install-command.test.ts` | `services/src/secret_provider.rs` |
| `adapter-utils/src/sandbox-install-command.ts` | `services/src/secret_provider_config_service.rs` |
| `adapter-utils/src/sandbox-managed-runtime.test.ts` | `services/src/secret_remote_import_service.rs` |
| `adapter-utils/src/sandbox-managed-runtime.ts` | `services/src/secret_service.rs` |
| `adapter-utils/src/sandbox-run-log-stream.ts` | `services/src/server_adapter.rs` |
| `adapter-utils/src/sandbox-shell.ts` | `services/src/user_secret_definition_service.rs` |
| `adapter-utils/src/server-utils-env.test.ts` | `services/src/user_secret_service.rs` |
| `adapter-utils/src/server-utils.test.ts` | `services/tests/adapter_runtime_credentials_test.rs` |
| `adapter-utils/src/server-utils.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/session-compaction.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/setup-token-transport.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/setup-token-transport.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/ssh-fixture.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/ssh.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/test-support/mcp-isolation-harness.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/types.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/workspace-restore-merge.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/workspace-restore-merge.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/cli/quota-probe.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/acp.auth.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/acp.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/acp.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/auth-check.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/claude-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/claude-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/cli-capabilities.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/config-schema.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/execute.acp-fallback.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/models.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/permissions.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/permissions.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/probe-diagnostics.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/probe-diagnostics.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/probe-redaction.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/prompt-cache.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/quota.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/setup-token-characterization.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/setup-token-parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/setup-token-parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/setup-token-runner.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/setup-token-runner.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/test.probe.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/test.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/claude-local/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/cli/quota-probe.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/index.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/acp.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/acp.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/adapter-auth-promotion.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/adapter-auth-promotion.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/auth-check.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/auth-precedence.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/auth-precedence.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-args.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-args.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-cache.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-cache.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-copyback.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-copyback.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-merge-decision.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-merge-decision.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-merge-scripts.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-merge.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-auth-seed-write.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-home.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/codex-home.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/config-schema.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-export.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-export.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-runner.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/device-login-runner.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.acp-fallback.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.auth-precedence.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.auth.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.stderr-error.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/output-inactivity-monitor.integration.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/output-inactivity-monitor.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/output-inactivity-monitor.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/process-activity-monitor.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/process-activity-monitor.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/quota-spawn-error.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/quota.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/runtime-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/runtime-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/test.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/ui/parse-stdout.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/codex-local/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/session.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/session.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/ui/parse-stdout.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-cloud/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/remote-command.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/remote-command.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/test.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/shared/stream.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/shared/trust.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/cursor-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/acp.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/acp.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/config-schema.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/execute.acp-fallback.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/server/utils.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/ui/parse-stdout.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/gemini-local/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/cli/format-event.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/test.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/shared/turn-boundary.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/shared/turn-boundary.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/ui/parse-stdout.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/grok-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes-gateway/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes-gateway/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes-gateway/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes-gateway/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes-gateway/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/config-schema.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/server/transport-security.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/shared/constants.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/gateway/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/index.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/command-resolution.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/config-schema.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/detect-model.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/detect-model.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/execute.onspawn.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/paperclip-task-bridge.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/prompt-rendering.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/shared/constants.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/ui/parse-stdout.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/hermes/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/shared/stream.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/ui/build-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/openclaw-gateway/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/index.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/execute.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/models.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/models.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/runtime-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/runtime-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/test.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/opencode-local/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/cli/format-event.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/cli/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/execute.remote.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/execute.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/models.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/models.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/parse.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/parse.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/runtime-config.test.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/runtime-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/skills.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/server/test.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/ui/build-config.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/ui/index.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/src/ui/parse-stdout.ts` | *(no structural counterpart in this slice)* |
| `adapters/pi-local/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `db/src/adapter-auth-sessions-schema.test.ts` | *(no structural counterpart in this slice)* |
| `db/src/company-secret-proposals-migration.test.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/adapter_auth_sessions.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/company_secret_bindings.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/company_secret_proposals.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/company_secret_provider_configs.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/company_secret_versions.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/company_secrets.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/secret_access_events.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/user_secret_declarations.ts` | *(no structural counterpart in this slice)* |
| `db/src/schema/user_secret_definitions.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/paperclip-plugin-fake-sandbox/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/auth.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/auth.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/exec.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/exec.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/helpers.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/routes.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/routes.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/sandboxes.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/sandboxes.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/src/sessions.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/bridge-template/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/bridge-client.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/bridge-client.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/types.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/cloudflare/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/file-sync.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/setup-token-pty.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/setup-token-pty.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/daytona/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/e2b.d.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/e2b/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/exe-dev/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/adapter-defaults.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/adapter-registry.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/cilium-network-policy.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/file-sync.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/image-allowlist.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/job-orchestrator.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/kube-client.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/lease-lifecycle.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/network-policy.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/pod-exec.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/pod-spec-builder.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/sandbox-cr-builder.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/sandbox-cr-orchestrator.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/sandbox-orchestrator.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/scoped-network-egress.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/secret-manager.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/tenant-orchestrator.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/types.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/upload-interceptor.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/utils.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/integration/_kind-harness.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/integration/end-to-end-run.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/adapter-defaults.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/cilium-network-policy.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/file-sync.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/image-allowlist.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/job-orchestrator.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/kube-client.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/lease-lifecycle.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/network-policy.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/plugin-lease-lifecycle.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/pod-exec.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/pod-spec-builder.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/sandbox-cr-builder.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/sandbox-cr-orchestrator.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/scoped-network-egress.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/secret-manager.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/tenant-orchestrator.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/types.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/utils.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/test/unit/wrap-command-with-env.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/kubernetes/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/modal/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/src/index.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/src/manifest.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/src/plugin.test.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/src/plugin.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/src/worker.ts` | *(no structural counterpart in this slice)* |
| `plugins/sandbox-providers/novita/vitest.config.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/adapter-agnostic-keys.test.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/adapter-type.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/adapter-types.test.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/types/adapter-registry.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/types/adapter-skills.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/types/secrets.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/validators/adapter-registry.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/validators/adapter-registry.test.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/validators/adapter-skills.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/validators/secret.d.ts` | *(no structural counterpart in this slice)* |
| `shared/dist/validators/secret.test.d.ts` | *(no structural counterpart in this slice)* |
| `shared/src/adapter-agnostic-keys.test.ts` | *(no structural counterpart in this slice)* |
| `shared/src/adapter-auth-session.ts` | *(no structural counterpart in this slice)* |
| `shared/src/adapter-type.ts` | *(no structural counterpart in this slice)* |
| `shared/src/adapter-types.test.ts` | *(no structural counterpart in this slice)* |
| `shared/src/types/adapter-registry.ts` | *(no structural counterpart in this slice)* |
| `shared/src/types/adapter-skills.ts` | *(no structural counterpart in this slice)* |
| `shared/src/types/agent.adapter-auth-session.test.ts` | *(no structural counterpart in this slice)* |
| `shared/src/types/secrets.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/adapter-auth-session.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/adapter-registry.test.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/adapter-registry.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/adapter-skills.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/secret.test.ts` | *(no structural counterpart in this slice)* |
| `shared/src/validators/secret.ts` | *(no structural counterpart in this slice)* |
