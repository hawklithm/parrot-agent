# Paperclip / Parrot Component Inventory

自动生成：`scripts/generate_parity_inventory.py`。

该清单用于 M0 的结构差异定位；不能替代 API 行为、权限、Schema、E2E 或视觉验收。

## CLI

CLI 归属已固定为 `parrot-agent/crates/cli`；后续需逐命令核对参数、认证、退出码和输出。

- Paperclip: **97**
- Parrot: **7**

| Paperclip evidence | Parrot evidence |
|---|---|
| `src/adapters/http/format-event.ts` | `Cargo.toml` |
| `src/adapters/http/index.ts` | `src/checks.rs` |
| `src/adapters/index.ts` | `src/client.rs` |
| `src/adapters/process/format-event.ts` | `src/commands.rs` |
| `src/adapters/process/index.ts` | `src/config.rs` |
| `src/adapters/registry.ts` | `src/main.rs` |
| `src/checks/agent-jwt-secret-check.ts` | `src/services.rs` |
| `src/checks/config-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/database-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/deployment-auth-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/index.ts` | *(no structural counterpart in this slice)* |
| `src/checks/llm-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/log-check.ts` | *(no structural counterpart in this slice)* |
| `src/checks/managed-install-check.ts` | *(no structural counterpart in this slice)* |
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
- Parrot: **485**

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
| `components/AgentIconPicker.tsx` | `components/AgentMultiSelect.tsx` |
| `components/AgentMultiSelect.test.tsx` | `components/AgentProperties.tsx` |
| `components/AgentMultiSelect.tsx` | `components/BootstrapPendingPage.tsx` |
| `components/AgentProperties.tsx` | `components/BudgetSidebarMarker.tsx` |
| `components/AgentSecretAccessEditor.test.tsx` | `components/BuiltInAgentBadges.tsx` |
| `components/AgentSecretAccessEditor.tsx` | `components/BuiltInAgentGate.test.tsx` |
| `components/AppConnectionSidebar.test.tsx` | `components/BuiltInAgentGate.tsx` |
| `components/AppConnectionSidebar.tsx` | `components/CompanySettingsSidebar.test.tsx` |
| `components/AppsSidebar.test.tsx` | `components/CompanySettingsSidebar.tsx` |
| `components/AppsSidebar.tsx` | `components/ConfigureBuiltInAgentModal.test.tsx` |
| `components/BootstrapPendingPage.tsx` | `components/ConfigureBuiltInAgentModal.tsx` |
| `components/BudgetSidebarMarker.tsx` | `components/InstanceSidebar.test.tsx` |
| `components/BuiltInAgentBadges.tsx` | `components/InstanceSidebar.tsx` |
| `components/BuiltInAgentGate.test.tsx` | `components/IssueAssignedBacklogNotice.test.tsx` |
| `components/BuiltInAgentGate.tsx` | `components/IssueAssignedBacklogNotice.tsx` |
| `components/CompanySettingsSidebar.test.tsx` | `components/IssueAttachmentsSection.test.tsx` |
| `components/CompanySettingsSidebar.tsx` | `components/IssueAttachmentsSection.tsx` |
| `components/ConfigureBuiltInAgentModal.test.tsx` | `components/IssueBlockedNotice.test.tsx` |
| `components/ConfigureBuiltInAgentModal.tsx` | `components/IssueBlockedNotice.tsx` |
| `components/InboxAgentPolicyControl.test.tsx` | `components/IssueCasesPanel.test.tsx` |
| `components/InboxAgentPolicyControl.tsx` | `components/IssueCasesPanel.tsx` |
| `components/InstanceSidebar.test.tsx` | `components/IssueChatComposerHandoffPreview.test.ts` |
| `components/InstanceSidebar.tsx` | `components/IssueChatThread.test.tsx` |
| `components/IssueAssignedBacklogNotice.test.tsx` | `components/IssueChatThread.tsx` |
| `components/IssueAssignedBacklogNotice.tsx` | `components/IssueChatThreadSystemNotice.test.tsx` |
| `components/IssueAttachmentsSection.test.tsx` | `components/IssueColumns.test.tsx` |
| `components/IssueAttachmentsSection.tsx` | `components/IssueColumns.tsx` |
| `components/IssueBlockedNotice.test.tsx` | `components/IssueContinuationHandoff.test.tsx` |
| `components/IssueBlockedNotice.tsx` | `components/IssueContinuationHandoff.tsx` |
| `components/IssueCasesPanel.test.tsx` | `components/IssueDocumentAnnotations.test.tsx` |
| `components/IssueCasesPanel.tsx` | `components/IssueDocumentAnnotations.tsx` |
| `components/IssueChatComposerHandoffPreview.test.ts` | `components/IssueDocumentsSection.test.tsx` |
| `components/IssueChatThread.test.tsx` | `components/IssueDocumentsSection.tsx` |
| `components/IssueChatThread.tsx` | `components/IssueFiltersPopover.test.tsx` |
| `components/IssueChatThreadSystemNotice.test.tsx` | `components/IssueFiltersPopover.tsx` |
| `components/IssueColumns.test.tsx` | `components/IssueGroupHeader.tsx` |
| `components/IssueColumns.tsx` | `components/IssueLinkQuicklook.test.tsx` |
| `components/IssueContinuationHandoff.test.tsx` | `components/IssueLinkQuicklook.tsx` |
| `components/IssueContinuationHandoff.tsx` | `components/IssueMonitorActivityCard.test.tsx` |
| `components/IssueDocumentAnnotations.test.tsx` | `components/IssueMonitorActivityCard.tsx` |
| `components/IssueDocumentAnnotations.tsx` | `components/IssueMonitorBanner.tsx` |
| `components/IssueDocumentsSection.test.tsx` | `components/IssuePlanDecompositionsSection.tsx` |
| `components/IssueDocumentsSection.tsx` | `components/IssueProperties.test.tsx` |
| `components/IssueFieldChangeReceipt.test.tsx` | `components/IssueProperties.tsx` |
| `components/IssueFieldChangeReceipt.tsx` | `components/IssueRecoveryActionCard.test.tsx` |
| `components/IssueFiltersPopover.test.tsx` | `components/IssueRecoveryActionCard.tsx` |
| `components/IssueFiltersPopover.tsx` | `components/IssueReferenceActivitySummary.tsx` |
| `components/IssueGroupHeader.tsx` | `components/IssueReferencePill.tsx` |
| `components/IssueLinkQuicklook.test.tsx` | `components/IssueRelatedWorkPanel.test.tsx` |
| `components/IssueLinkQuicklook.tsx` | `components/IssueRelatedWorkPanel.tsx` |
| `components/IssueMonitorBanner.test.tsx` | `components/IssueRow.test.tsx` |
| `components/IssueMonitorBanner.tsx` | `components/IssueRow.tsx` |
| `components/IssuePlanDecompositionsSection.tsx` | `components/IssueRunLedger.test.tsx` |
| `components/IssueProperties.test.tsx` | `components/IssueRunLedger.tsx` |
| `components/IssueProperties.tsx` | `components/IssueScheduledRetryCard.test.tsx` |
| `components/IssueRecoveryActionCard.test.tsx` | `components/IssueScheduledRetryCard.tsx` |
| `components/IssueRecoveryActionCard.tsx` | `components/IssueSiblingNavigation.test.tsx` |
| `components/IssueReferenceActivitySummary.tsx` | `components/IssueSiblingNavigation.tsx` |
| `components/IssueReferencePill.tsx` | `components/IssueThreadInteractionCard.test.tsx` |
| `components/IssueRelatedWorkPanel.test.tsx` | `components/IssueThreadInteractionCard.tsx` |
| `components/IssueRelatedWorkPanel.tsx` | `components/IssueWorkspaceCard.test.tsx` |
| `components/IssueRow.test.tsx` | `components/IssueWorkspaceCard.tsx` |
| `components/IssueRow.tsx` | `components/IssuesList.test.tsx` |
| `components/IssueRunLedger.test.tsx` | `components/IssuesList.tsx` |
| `components/IssueRunLedger.tsx` | `components/IssuesQuicklook.tsx` |
| `components/IssueScheduledRetryCard.test.tsx` | `components/NewAgentDialog.test.tsx` |
| `components/IssueScheduledRetryCard.tsx` | `components/NewAgentDialog.tsx` |
| `components/IssueSiblingNavigation.test.tsx` | `components/NewIssueDialog.test.tsx` |
| `components/IssueSiblingNavigation.tsx` | `components/NewIssueDialog.tsx` |
| `components/IssueThreadInteractionCard.test.tsx` | `components/PageSkeleton.tsx` |
| `components/IssueThreadInteractionCard.tsx` | `components/PageTabBar.tsx` |
| `components/IssueWorkspaceCard.test.tsx` | `components/RequestCollapsedSidebar.test.tsx` |
| `components/IssueWorkspaceCard.tsx` | `components/RequestCollapsedSidebar.tsx` |
| `components/IssueWriteDenialNotice.test.tsx` | `components/RouteErrorBoundary.test.tsx` |
| `components/IssueWriteDenialNotice.tsx` | `components/RouteErrorBoundary.tsx` |
| `components/IssuesList.test.tsx` | `components/RoutineSubSidebar.test.tsx` |
| `components/IssuesList.tsx` | `components/RoutineSubSidebar.tsx` |
| `components/IssuesQuicklook.tsx` | `components/SecondarySidebar.tsx` |
| `components/NewAgentDialog.test.tsx` | `components/Sidebar.test.tsx` |
| `components/NewAgentDialog.tsx` | `components/Sidebar.tsx` |
| `components/NewIssueDialog.test.tsx` | `components/SidebarAccountMenu.test.tsx` |
| `components/NewIssueDialog.tsx` | `components/SidebarAccountMenu.tsx` |
| `components/PageSkeleton.tsx` | `components/SidebarAgents.test.tsx` |
| `components/PageTabBar.tsx` | `components/SidebarAgents.tsx` |
| `components/RequestCollapsedSidebar.test.tsx` | `components/SidebarCompanyMenu.test.tsx` |
| `components/RequestCollapsedSidebar.tsx` | `components/SidebarCompanyMenu.tsx` |
| `components/RouteErrorBoundary.test.tsx` | `components/SidebarNavItem.test.tsx` |
| `components/RouteErrorBoundary.tsx` | `components/SidebarNavItem.tsx` |
| `components/RoutineSubSidebar.test.tsx` | `components/SidebarProjects.test.tsx` |
| `components/RoutineSubSidebar.tsx` | `components/SidebarProjects.tsx` |
| `components/SecondarySidebar.tsx` | `components/SidebarSection.test.tsx` |
| `components/Sidebar.test.tsx` | `components/SidebarSection.tsx` |
| `components/Sidebar.tsx` | `components/SidebarServerInfo.test.tsx` |
| `components/SidebarAccountMenu.test.tsx` | `components/SidebarServerInfo.tsx` |
| `components/SidebarAccountMenu.tsx` | `components/SidebarShell.test.tsx` |
| `components/SidebarAgents.test.tsx` | `components/SidebarShell.tsx` |
| `components/SidebarAgents.tsx` | `components/SidebarStarredProjects.test.tsx` |
| `components/SidebarCompanyMenu.test.tsx` | `components/SidebarStarredProjects.tsx` |
| `components/SidebarCompanyMenu.tsx` | `components/access/CompanySettingsNav.test.tsx` |
| `components/SidebarNavItem.test.tsx` | `components/access/CompanySettingsNav.tsx` |
| `components/SidebarNavItem.tsx` | `components/agent-config-defaults.ts` |
| `components/SidebarProjects.test.tsx` | `components/agent-config-primitives.tsx` |
| `components/SidebarProjects.tsx` | `components/issue-output/IssueOutputSection.test.tsx` |
| `components/SidebarSection.test.tsx` | `components/issue-output/IssueOutputSection.tsx` |
| `components/SidebarSection.tsx` | `components/issue-output/OutputFileTile.tsx` |
| `components/SidebarServerInfo.test.tsx` | `components/issue-output/OutputPrimaryCard.tsx` |
| `components/SidebarServerInfo.tsx` | `components/issue-output/OutputRow.tsx` |
| `components/SidebarShell.test.tsx` | `components/issue-output/OutputVideoPlayer.tsx` |
| `components/SidebarShell.tsx` | `components/issue-properties/IssueProperties.tsx` |
| `components/SidebarStarredProjects.test.tsx` | `components/issue-properties/external-object-rows.tsx` |
| `components/SidebarStarredProjects.tsx` | `components/issue-properties/helpers.ts` |
| `components/access/CompanySettingsNav.test.tsx` | `components/issue-properties/index.ts` |
| `components/access/CompanySettingsNav.tsx` | `components/issue-properties/primitives.tsx` |
| `components/agent-config-defaults.ts` | `components/issue-properties/property-picker.tsx` |
| `components/agent-config-primitives.tsx` | `components/issue-properties/relation-controls.tsx` |
| `components/issue-output/IssueOutputSection.test.tsx` | `components/skill-studio/AgentsUsingSkillDialog.test.tsx` |
| `components/issue-output/IssueOutputSection.tsx` | `components/skill-studio/AgentsUsingSkillDialog.tsx` |
| `components/issue-output/OutputFileTile.tsx` | `context/GeneralSettingsContext.tsx` |
| `components/issue-output/OutputPrimaryCard.tsx` | `context/SidebarContext.test.tsx` |
| `components/issue-output/OutputRow.tsx` | `context/SidebarContext.tsx` |
| `components/issue-output/OutputVideoPlayer.tsx` | `fixtures/issueChatLongThreadFixture.test.ts` |
| `components/issue-properties/IssueProperties.tsx` | `fixtures/issueChatLongThreadFixture.ts` |
| `components/issue-properties/IssuePropertiesArtifactsTab.tsx` | `fixtures/issueChatUxFixtures.ts` |
| `components/issue-properties/IssuePropertiesDocumentAnnotations.test.tsx` | `fixtures/issueThreadInteractionFixtures.ts` |
| `components/issue-properties/IssuePropertiesPlansTab.tsx` | `hooks/useAgentOrder.ts` |
| `components/issue-properties/external-object-rows.tsx` | `hooks/useCompanyPageMemory.test.ts` |
| `components/issue-properties/helpers.ts` | `hooks/useCompanyPageMemory.ts` |
| `components/issue-properties/index.ts` | `hooks/useIssueExternalObjects.ts` |
| `components/issue-properties/primitives.tsx` | `hooks/usePaperclipIssueRuntime.test.tsx` |
| `components/issue-properties/property-picker.tsx` | `hooks/usePaperclipIssueRuntime.ts` |
| `components/issue-properties/relation-controls.tsx` | `lib/agent-config-patch.test.ts` |
| `components/skill-studio/AgentsUsingSkillDialog.test.tsx` | `lib/agent-config-patch.ts` |
| `components/skill-studio/AgentsUsingSkillDialog.tsx` | `lib/agent-icons.ts` |
| `context/GeneralSettingsContext.tsx` | `lib/agent-onboarding-prompt.test.ts` |
| `context/SidebarContext.test.tsx` | `lib/agent-onboarding-prompt.ts` |
| `context/SidebarContext.tsx` | `lib/agent-order.test.ts` |
| `fixtures/issueChatLongThreadFixture.test.ts` | `lib/agent-order.ts` |
| `fixtures/issueChatLongThreadFixture.ts` | `lib/agent-skills-state.test.ts` |
| `fixtures/issueChatUxFixtures.ts` | `lib/agent-skills-state.ts` |
| `fixtures/issueThreadInteractionFixtures.ts` | `lib/built-in-agent-toast.ts` |
| `hooks/useAgentOrder.ts` | `lib/company-page-memory.ts` |
| `hooks/useCompanyPageMemory.test.ts` | `lib/company-portability-sidebar.test.ts` |
| `hooks/useCompanyPageMemory.ts` | `lib/company-portability-sidebar.ts` |
| `hooks/useIssueDocuments.ts` | `lib/company-routes.test.ts` |
| `hooks/useIssueExternalObjects.ts` | `lib/company-routes.ts` |
| `hooks/useIssuePlanDocument.ts` | `lib/company-skill-routes.test.ts` |
| `hooks/usePaperclipIssueRuntime.test.tsx` | `lib/company-skill-routes.ts` |
| `hooks/usePaperclipIssueRuntime.ts` | `lib/duplicate-agent-payload.test.ts` |
| `lib/agent-config-patch.test.ts` | `lib/duplicate-agent-payload.ts` |
| `lib/agent-config-patch.ts` | `lib/instance-settings.test.ts` |
| `lib/agent-icons.ts` | `lib/instance-settings.ts` |
| `lib/agent-onboarding-prompt.test.ts` | `lib/issue-assignee-overrides.test.ts` |
| `lib/agent-onboarding-prompt.ts` | `lib/issue-assignee-overrides.ts` |
| `lib/agent-order.test.ts` | `lib/issue-attachments.ts` |
| `lib/agent-order.ts` | `lib/issue-blockers.ts` |
| `lib/agent-skills-state.test.ts` | `lib/issue-chat-messages.test.ts` |
| `lib/agent-skills-state.ts` | `lib/issue-chat-messages.ts` |
| `lib/built-in-agent-toast.ts` | `lib/issue-chat-scroll.test.ts` |
| `lib/company-page-memory.ts` | `lib/issue-chat-scroll.ts` |
| `lib/company-portability-sidebar.test.ts` | `lib/issue-detail-subissues.test.ts` |
| `lib/company-portability-sidebar.ts` | `lib/issue-detail-subissues.ts` |
| `lib/company-routes.test.ts` | `lib/issue-execution-policy.test.ts` |
| `lib/company-routes.ts` | `lib/issue-execution-policy.ts` |
| `lib/company-skill-routes.test.ts` | `lib/issue-filters.test.ts` |
| `lib/company-skill-routes.ts` | `lib/issue-filters.ts` |
| `lib/duplicate-agent-payload.test.ts` | `lib/issue-monitor.ts` |
| `lib/duplicate-agent-payload.ts` | `lib/issue-output.test.ts` |
| `lib/instance-settings.test.ts` | `lib/issue-output.ts` |
| `lib/instance-settings.ts` | `lib/issue-properties-panel-key.test.ts` |
| `lib/issue-artifacts.test.ts` | `lib/issue-properties-panel-key.ts` |
| `lib/issue-artifacts.ts` | `lib/issue-reference.test.ts` |
| `lib/issue-assignee-overrides.test.ts` | `lib/issue-reference.ts` |
| `lib/issue-assignee-overrides.ts` | `lib/issue-thread-interactions.test.ts` |
| `lib/issue-attachments.ts` | `lib/issue-thread-interactions.ts` |
| `lib/issue-blockers.ts` | `lib/issue-timeline-events.test.ts` |
| `lib/issue-change-receipt.test.ts` | `lib/issue-timeline-events.ts` |
| `lib/issue-change-receipt.ts` | `lib/issue-tree.test.ts` |
| `lib/issue-chat-messages.test.ts` | `lib/issue-tree.ts` |
| `lib/issue-chat-messages.ts` | `lib/issueActiveRun.test.ts` |
| `lib/issue-chat-scroll.test.ts` | `lib/issueActiveRun.ts` |
| `lib/issue-chat-scroll.ts` | `lib/issueChatTranscriptRuns.test.ts` |
| `lib/issue-detail-performance.ts` | `lib/issueChatTranscriptRuns.ts` |
| `lib/issue-detail-subissues.test.ts` | `lib/issueDetailBreadcrumb.test.ts` |
| `lib/issue-detail-subissues.ts` | `lib/issueDetailBreadcrumb.ts` |
| `lib/issue-document-deep-link.test.ts` | `lib/issueDetailCache.test.ts` |
| `lib/issue-document-deep-link.ts` | `lib/issueDetailCache.ts` |
| `lib/issue-execution-policy.test.ts` | `lib/issueDetailQuery.test.tsx` |
| `lib/issue-execution-policy.ts` | `lib/legacy-agent-config.test.ts` |
| `lib/issue-filters.test.ts` | `lib/legacy-agent-config.ts` |
| `lib/issue-filters.ts` | `lib/liveIssueIds.test.ts` |
| `lib/issue-monitor.test.tsx` | `lib/liveIssueIds.ts` |
| `lib/issue-monitor.ts` | `lib/new-agent-hire-payload.test.ts` |
| `lib/issue-output.test.ts` | `lib/new-agent-hire-payload.ts` |
| `lib/issue-output.ts` | `lib/new-agent-runtime-config.test.ts` |
| `lib/issue-properties-panel-key.test.ts` | `lib/new-agent-runtime-config.ts` |
| `lib/issue-properties-panel-key.ts` | `lib/onboarding-route.test.ts` |
| `lib/issue-reference.test.ts` | `lib/onboarding-route.ts` |
| `lib/issue-reference.ts` | `lib/optimistic-issue-comments.test.ts` |
| `lib/issue-thread-interactions.test.ts` | `lib/optimistic-issue-comments.ts` |
| `lib/issue-thread-interactions.ts` | `lib/optimistic-issue-runs.test.ts` |
| `lib/issue-timeline-events.test.ts` | `lib/optimistic-issue-runs.ts` |
| `lib/issue-timeline-events.ts` | `lib/page-visibility.test.ts` |
| `lib/issue-tree.test.ts` | `lib/page-visibility.ts` |
| `lib/issue-tree.ts` | `lib/paperclip-shared/src/agent-eligibility.test.ts` |
| `lib/issue-write-denial-activity.ts` | `lib/paperclip-shared/src/agent-eligibility.ts` |
| `lib/issueActiveRun.test.ts` | `lib/paperclip-shared/src/agent-url-key.ts` |
| `lib/issueActiveRun.ts` | `lib/paperclip-shared/src/issue-attribution.test.ts` |
| `lib/issueChatTranscriptRuns.test.ts` | `lib/paperclip-shared/src/issue-attribution.ts` |
| `lib/issueChatTranscriptRuns.ts` | `lib/paperclip-shared/src/issue-references.test.ts` |
| `lib/issueDetailBreadcrumb.test.ts` | `lib/paperclip-shared/src/issue-references.ts` |
| `lib/issueDetailBreadcrumb.ts` | `lib/paperclip-shared/src/issue-thread-interactions.test.ts` |
| `lib/issueDetailCache.test.ts` | `lib/paperclip-shared/src/types/agent.ts` |
| `lib/issueDetailCache.ts` | `lib/paperclip-shared/src/types/issue-tree-control.ts` |
| `lib/issueDetailQuery.test.tsx` | `lib/paperclip-shared/src/types/issue.ts` |
| `lib/legacy-agent-config.test.ts` | `lib/paperclip-shared/src/types/sidebar-badges.ts` |
| `lib/legacy-agent-config.ts` | `lib/paperclip-shared/src/types/sidebar-preferences.ts` |
| `lib/liveIssueIds.test.ts` | `lib/paperclip-shared/src/validators/agent.ts` |
| `lib/liveIssueIds.ts` | `lib/paperclip-shared/src/validators/issue-tree-control.ts` |
| `lib/new-agent-hire-payload.test.ts` | `lib/paperclip-shared/src/validators/issue.test.ts` |
| `lib/new-agent-hire-payload.ts` | `lib/paperclip-shared/src/validators/issue.ts` |
| `lib/new-agent-runtime-config.test.ts` | `lib/paperclip-shared/src/validators/sidebar-preferences.ts` |
| `lib/new-agent-runtime-config.ts` | `lib/paperclip-tools-shared/agent-eligibility.test.ts` |
| `lib/onboarding-route.test.ts` | `lib/paperclip-tools-shared/agent-eligibility.ts` |
| `lib/onboarding-route.ts` | `lib/paperclip-tools-shared/agent-url-key.ts` |
| `lib/optimistic-issue-comments.test.ts` | `lib/paperclip-tools-shared/issue-attribution.test.ts` |
| `lib/optimistic-issue-comments.ts` | `lib/paperclip-tools-shared/issue-attribution.ts` |
| `lib/optimistic-issue-runs.test.ts` | `lib/paperclip-tools-shared/issue-references.test.ts` |
| `lib/optimistic-issue-runs.ts` | `lib/paperclip-tools-shared/issue-references.ts` |
| `lib/page-visibility.test.ts` | `lib/paperclip-tools-shared/issue-thread-interactions.test.ts` |
| `lib/page-visibility.ts` | `lib/paperclip-tools-shared/issue-write-denial.test.ts` |
| `lib/prefetchIssueComments.test.ts` | `lib/paperclip-tools-shared/issue-write-denial.ts` |
| `lib/router.tsx` | `lib/paperclip-tools-shared/types/agent.adapter-auth-session.test.ts` |
| `lib/subIssueDefaults.test.ts` | `lib/paperclip-tools-shared/types/agent.ts` |
| `lib/subIssueDefaults.ts` | `lib/paperclip-tools-shared/types/inbox-agent-policy.ts` |
| `pages/AdapterManager.tsx` | `lib/paperclip-tools-shared/types/issue-tree-control.ts` |
| `pages/AgentDetail.instructions.test.tsx` | `lib/paperclip-tools-shared/types/issue.ts` |
| `pages/AgentDetail.liveRun.test.ts` | `lib/paperclip-tools-shared/types/sidebar-badges.ts` |
| `pages/AgentDetail.progress.test.ts` | `lib/paperclip-tools-shared/types/sidebar-preferences.ts` |
| `pages/AgentDetail.tsx` | `lib/paperclip-tools-shared/validators/agent.ts` |
| `pages/AgentToolsTab.test.tsx` | `lib/paperclip-tools-shared/validators/inbox-agent-policy.ts` |
| `pages/AgentToolsTab.tsx` | `lib/paperclip-tools-shared/validators/issue-tree-control.ts` |
| `pages/Agents.test.tsx` | `lib/paperclip-tools-shared/validators/issue.test.ts` |
| `pages/Agents.tsx` | `lib/paperclip-tools-shared/validators/issue.ts` |
| `pages/ApprovalDetail.tsx` | `lib/paperclip-tools-shared/validators/sidebar-preferences.ts` |
| `pages/Approvals.tsx` | `lib/router.tsx` |
| `pages/Artifacts.test.tsx` | `lib/subIssueDefaults.test.ts` |
| `pages/Artifacts.tsx` | `lib/subIssueDefaults.ts` |
| `pages/Auth.test.tsx` | `pages/Activity.tsx` |
| `pages/Auth.tsx` | `pages/AdapterManager.tsx` |
| `pages/BoardChat.test.tsx` | `pages/AgentDetail.instructions.test.tsx` |
| `pages/BoardChat.tsx` | `pages/AgentDetail.progress.test.ts` |
| `pages/BoardClaim.tsx` | `pages/AgentDetail.tsx` |
| `pages/BootstrapSetupUxLab.tsx` | `pages/AgentToolsTab.tsx` |
| `pages/CaseDetail.test.tsx` | `pages/Agents.test.tsx` |
| `pages/CaseDetail.tsx` | `pages/Agents.tsx` |
| `pages/Cases.test.tsx` | `pages/ApprovalDetail.tsx` |
| `pages/Cases.tsx` | `pages/Approvals.tsx` |
| `pages/CliAuth.tsx` | `pages/Artifacts.test.tsx` |
| `pages/Companies.test.tsx` | `pages/Artifacts.tsx` |
| `pages/Companies.tsx` | `pages/Auth.test.tsx` |
| `pages/CompanyAccess.test.tsx` | `pages/Auth.tsx` |
| `pages/CompanyAccess.tsx` | `pages/BoardChat.test.tsx` |
| `pages/CompanyEnvironments.test.tsx` | `pages/BoardChat.tsx` |
| `pages/CompanyEnvironments.tsx` | `pages/BoardClaim.tsx` |
| `pages/CompanyExport.test.tsx` | `pages/BootstrapSetupUxLab.tsx` |
| `pages/CompanyExport.tsx` | `pages/CaseDetail.test.tsx` |
| `pages/CompanyImport.test.tsx` | `pages/CaseDetail.tsx` |
| `pages/CompanyImport.tsx` | `pages/Cases.test.tsx` |
| `pages/CompanyInvites.test.tsx` | `pages/Cases.tsx` |
| `pages/CompanyInvites.tsx` | `pages/CliAuth.tsx` |
| `pages/CompanySettings.test.tsx` | `pages/CloudUpstream.test.tsx` |
| `pages/CompanySettings.tsx` | `pages/CloudUpstream.tsx` |
| `pages/CompanySettingsPluginPage.test.tsx` | `pages/CloudUpstreamUxLab.tsx` |
| `pages/CompanySettingsPluginPage.tsx` | `pages/Companies.tsx` |
| `pages/CompanySkills.test.tsx` | `pages/CompanyAccess.test.tsx` |
| `pages/CompanySkills.tsx` | `pages/CompanyAccess.tsx` |
| `pages/Costs.tsx` | `pages/CompanyEnvironments.test.tsx` |
| `pages/CrossIssueCollaborationUxLab.tsx` | `pages/CompanyEnvironments.tsx` |
| `pages/Dashboard.tsx` | `pages/CompanyExport.tsx` |
| `pages/DashboardLive.tsx` | `pages/CompanyImport.tsx` |
| `pages/DecisionQueuePage.tsx` | `pages/CompanyInvites.test.tsx` |
| `pages/DesignGuide.tsx` | `pages/CompanyInvites.tsx` |
| `pages/ExecutionWorkspaceDetail.provision-status.test.ts` | `pages/CompanySettings.test.tsx` |
| `pages/ExecutionWorkspaceDetail.service-ports.test.ts` | `pages/CompanySettings.tsx` |
| `pages/ExecutionWorkspaceDetail.test.tsx` | `pages/CompanySettingsPluginPage.test.tsx` |
| `pages/ExecutionWorkspaceDetail.tsx` | `pages/CompanySettingsPluginPage.tsx` |
| `pages/GoalDetail.test.tsx` | `pages/CompanySkills.test.tsx` |
| `pages/GoalDetail.tsx` | `pages/CompanySkills.tsx` |
| `pages/Goals.tsx` | `pages/Costs.tsx` |
| `pages/Inbox.test.tsx` | `pages/Dashboard.tsx` |
| `pages/Inbox.tsx` | `pages/DashboardLive.tsx` |
| `pages/InstanceAccess.tsx` | `pages/DesignGuide.tsx` |
| `pages/InstanceExperimentalSettings.test.tsx` | `pages/ExecutionWorkspaceDetail.test.tsx` |
| `pages/InstanceExperimentalSettings.tsx` | `pages/ExecutionWorkspaceDetail.tsx` |
| `pages/InstanceGeneralSettings.test.tsx` | `pages/GoalDetail.test.tsx` |
| `pages/InstanceGeneralSettings.tsx` | `pages/GoalDetail.tsx` |
| `pages/InstanceSettings.tsx` | `pages/Goals.tsx` |
| `pages/InviteLanding.test.tsx` | `pages/Inbox.test.tsx` |
| `pages/InviteLanding.tsx` | `pages/Inbox.tsx` |
| `pages/InviteUxLab.test.tsx` | `pages/InstanceAccess.tsx` |
| `pages/InviteUxLab.tsx` | `pages/InstanceExperimentalSettings.test.tsx` |
| `pages/IssueChatLongThreadPerf.tsx` | `pages/InstanceExperimentalSettings.tsx` |
| `pages/IssueChatUxLab.tsx` | `pages/InstanceGeneralSettings.tsx` |
| `pages/IssueDetail.test.tsx` | `pages/InstanceSettings.tsx` |
| `pages/IssueDetail.tsx` | `pages/InviteLanding.test.tsx` |
| `pages/Issues.test.tsx` | `pages/InviteLanding.tsx` |
| `pages/Issues.tsx` | `pages/InviteUxLab.test.tsx` |
| `pages/JoinRequestQueue.tsx` | `pages/InviteUxLab.tsx` |
| `pages/MyIssues.tsx` | `pages/IssueChatLongThreadPerf.tsx` |
| `pages/NewAgent.test.tsx` | `pages/IssueChatUxLab.tsx` |
| `pages/NewAgent.tsx` | `pages/IssueDetail.test.tsx` |
| `pages/NotFound.tsx` | `pages/IssueDetail.tsx` |
| `pages/Org.tsx` | `pages/Issues.test.tsx` |
| `pages/OrgChart.test.tsx` | `pages/Issues.tsx` |
| `pages/OrgChart.tsx` | `pages/JoinRequestQueue.tsx` |
| `pages/PipelineSettings.test.ts` | `pages/MyIssues.tsx` |
| `pages/PipelineSettings.tsx` | `pages/NewAgent.tsx` |
| `pages/Pipelines.test.tsx` | `pages/NotFound.tsx` |
| `pages/Pipelines.tsx` | `pages/Org.tsx` |
| `pages/PluginManager.tsx` | `pages/OrgChart.test.tsx` |
| `pages/PluginPage.test.tsx` | `pages/OrgChart.tsx` |
| `pages/PluginPage.tsx` | `pages/PipelineSettings.test.ts` |
| `pages/PluginSettings.test.tsx` | `pages/PipelineSettings.tsx` |
| `pages/PluginSettings.tsx` | `pages/Pipelines.test.tsx` |
| `pages/ProfileSettings.test.tsx` | `pages/Pipelines.tsx` |
| `pages/ProfileSettings.tsx` | `pages/PluginManager.tsx` |
| `pages/ProjectDetail.test.tsx` | `pages/PluginPage.test.tsx` |
| `pages/ProjectDetail.tsx` | `pages/PluginPage.tsx` |
| `pages/ProjectWorkspaceDetail.test.tsx` | `pages/PluginSettings.test.tsx` |
| `pages/ProjectWorkspaceDetail.tsx` | `pages/PluginSettings.tsx` |
| `pages/Projects.test.tsx` | `pages/ProfileSettings.test.tsx` |
| `pages/Projects.tsx` | `pages/ProfileSettings.tsx` |
| `pages/ResponsibleUserDenialUxLab.tsx` | `pages/ProjectDetail.test.tsx` |
| `pages/RoutineDetail.test.tsx` | `pages/ProjectDetail.tsx` |
| `pages/RoutineDetail.tsx` | `pages/ProjectWorkspaceDetail.test.tsx` |
| `pages/Routines.test.tsx` | `pages/ProjectWorkspaceDetail.tsx` |
| `pages/Routines.tsx` | `pages/Projects.test.tsx` |
| `pages/RunTranscriptUxLab.tsx` | `pages/Projects.tsx` |
| `pages/Search.test.tsx` | `pages/ResponsibleUserDenialUxLab.tsx` |
| `pages/Search.tsx` | `pages/RoutineDetail.tsx` |
| `pages/Secrets.render.test.tsx` | `pages/Routines.test.tsx` |
| `pages/Secrets.test.ts` | `pages/Routines.tsx` |
| `pages/Secrets.tsx` | `pages/RunTranscriptUxLab.tsx` |
| `pages/SkillStudio.test.tsx` | `pages/Search.test.tsx` |
| `pages/SkillStudio.tsx` | `pages/Search.tsx` |
| `pages/StatusCards/ArchivedStatusCardRow.tsx` | `pages/Secrets.render.test.tsx` |
| `pages/StatusCards/CreateStatusCardDialog.tsx` | `pages/Secrets.test.ts` |
| `pages/StatusCards/StatusCardDetailDrawer.tsx` | `pages/Secrets.tsx` |
| `pages/StatusCards/StatusCardSettingsForm.test.tsx` | `pages/SkillStudio.test.tsx` |
| `pages/StatusCards/StatusCardSettingsForm.tsx` | `pages/SkillStudio.tsx` |
| `pages/StatusCards/StatusCardTile.test.tsx` | `pages/StatusCards.tsx` |
| `pages/StatusCards/StatusCardTile.tsx` | `pages/SystemNoticeUxLab.tsx` |
| `pages/StatusCards/SummarizerAgentSelect.tsx` | `pages/TeamCard.test.tsx` |
| `pages/StatusCards/format.test.ts` | `pages/TeamCatalog.fixtures.ts` |
| `pages/StatusCards/format.ts` | `pages/TeamCatalog.test.tsx` |
| `pages/StatusCards/index.tsx` | `pages/TeamCatalog.tsx` |
| `pages/StatusCards/types.ts` | `pages/Timeline.test.tsx` |
| `pages/SystemNoticeUxLab.tsx` | `pages/Timeline.tsx` |
| `pages/TaskChatLab.tsx` | `pages/ToolsCenter.tsx` |
| `pages/TeamCard.test.tsx` | `pages/UserProfile.tsx` |
| `pages/TeamCatalog.fixtures.ts` | `pages/WhatNeedsMe.tsx` |
| `pages/TeamCatalog.test.tsx` | `pages/Workspaces.test.tsx` |
| `pages/TeamCatalog.tsx` | `pages/Workspaces.tsx` |
| `pages/Timeline.test.tsx` | `pages/agent-skills/AgentSkillRow.tsx` |
| `pages/Timeline.tsx` | `pages/agent-skills/AgentSkillsTab.tsx` |
| `pages/UserProfile.tsx` | `pages/agent-skills/agent-skill-filter.test.ts` |
| `pages/WhatNeedsMe.test.tsx` | `pages/agent-skills/agent-skill-filter.ts` |
| `pages/WhatNeedsMe.tsx` | `pages/agent-skills/agent-skill-source.test.ts` |
| `pages/Workspaces.test.tsx` | `pages/agent-skills/agent-skill-source.ts` |
| `pages/Workspaces.tsx` | `pages/apps/AppDetail.test.tsx` |
| `pages/agent-skills/AgentSkillReleasePicker.test.ts` | `pages/apps/AppDetail.tsx` |
| `pages/agent-skills/AgentSkillReleasePicker.tsx` | `pages/apps/AppLogo.tsx` |
| `pages/agent-skills/AgentSkillRow.tsx` | `pages/apps/AppNotConnected.test.tsx` |
| `pages/agent-skills/AgentSkillsTab.test.ts` | `pages/apps/AppNotConnected.tsx` |
| `pages/agent-skills/AgentSkillsTab.tsx` | `pages/apps/AppsConnect.test.tsx` |
| `pages/agent-skills/agent-skill-filter.test.ts` | `pages/apps/AppsConnect.tsx` |
| `pages/agent-skills/agent-skill-filter.ts` | `pages/apps/AppsReview.tsx` |
| `pages/agent-skills/agent-skill-source.test.ts` | `pages/apps/Browse.test.tsx` |
| `pages/agent-skills/agent-skill-source.ts` | `pages/apps/Browse.tsx` |
| `pages/apps/AppDetail.test.tsx` | `pages/apps/Connections.test.tsx` |
| `pages/apps/AppDetail.tsx` | `pages/apps/Connections.tsx` |
| `pages/apps/AppLogo.tsx` | `pages/apps/ReviewQueueCard.test.tsx` |
| `pages/apps/AppNotConnected.test.tsx` | `pages/apps/ReviewQueueCard.tsx` |
| `pages/apps/AppNotConnected.tsx` | `pages/apps/app-connect-policy.test.ts` |
| `pages/apps/AppsConnect.test.tsx` | `pages/apps/app-connect-policy.ts` |
| `pages/apps/AppsConnect.tsx` | `pages/apps/app-definition-display.ts` |
| `pages/apps/AppsReview.tsx` | `pages/apps/app-detail/ActivityPanel.render.test.tsx` |
| `pages/apps/Browse.test.tsx` | `pages/apps/app-detail/ActivityPanel.test.tsx` |
| `pages/apps/Browse.tsx` | `pages/apps/app-detail/ActivityPanel.tsx` |
| `pages/apps/Connections.test.tsx` | `pages/apps/app-detail/AdvancedPanel.tsx` |
| `pages/apps/Connections.tsx` | `pages/apps/app-detail/PermissionsPanel.tsx` |
| `pages/apps/ReviewQueueCard.test.tsx` | `pages/apps/app-detail/ReviewPanel.tsx` |
| `pages/apps/ReviewQueueCard.tsx` | `pages/apps/app-detail/SetupPanel.tsx` |
| `pages/apps/app-connect-policy.test.ts` | `pages/apps/app-detail/TestPanel.test.tsx` |
| `pages/apps/app-connect-policy.ts` | `pages/apps/app-detail/TestPanel.tsx` |
| `pages/apps/app-definition-display.ts` | `pages/apps/app-detail/types.ts` |
| `pages/apps/app-detail/ActivityPanel.render.test.tsx` | `pages/apps/app-tabs.ts` |
| `pages/apps/app-detail/ActivityPanel.test.tsx` | `pages/apps/gateways/AppsSubNav.tsx` |
| `pages/apps/app-detail/ActivityPanel.tsx` | `pages/apps/gateways/ConnectClientDialog.tsx` |
| `pages/apps/app-detail/AdvancedPanel.tsx` | `pages/apps/gateways/GatewayDetail.tsx` |
| `pages/apps/app-detail/PermissionsPanel.tsx` | `pages/apps/gateways/GatewaysList.tsx` |
| `pages/apps/app-detail/ReviewPanel.tsx` | `pages/apps/gateways/NewGatewayDialog.tsx` |
| `pages/apps/app-detail/SetupPanel.tsx` | `pages/apps/gateways/gateway-helpers.test.ts` |
| `pages/apps/app-detail/TestPanel.test.tsx` | `pages/apps/gateways/gateway-helpers.ts` |
| `pages/apps/app-detail/TestPanel.tsx` | `pages/apps/gateways/gateway-tabs.ts` |
| `pages/apps/app-detail/types.ts` | `pages/apps/gateways/panels/AppsToolsPanel.tsx` |
| `pages/apps/app-tabs.ts` | `pages/apps/gateways/panels/GatewayActivityPanel.tsx` |
| `pages/apps/gateways/AppsSubNav.tsx` | `pages/apps/gateways/panels/GatewayAdvancedPanel.tsx` |
| `pages/apps/gateways/ConnectClientDialog.tsx` | `pages/apps/gateways/panels/OverviewPanel.tsx` |
| `pages/apps/gateways/GatewayDetail.tsx` | `pages/apps/gateways/panels/TokensPanel.test.tsx` |
| `pages/apps/gateways/GatewaysList.tsx` | `pages/apps/gateways/panels/TokensPanel.tsx` |
| `pages/apps/gateways/NewGatewayDialog.tsx` | `pages/apps/google-sheets.ts` |
| `pages/apps/gateways/gateway-helpers.test.ts` | `pages/apps/store-cards.tsx` |
| `pages/apps/gateways/gateway-helpers.ts` | `pages/apps/useReviewCount.ts` |
| `pages/apps/gateways/gateway-tabs.ts` | `pages/secrets/ImportFromVaultDialog.test.tsx` |
| `pages/apps/gateways/panels/AppsToolsPanel.tsx` | `pages/secrets/ImportFromVaultDialog.tsx` |
| `pages/apps/gateways/panels/GatewayActivityPanel.tsx` | `pages/secrets/MissingUserSecretsBanner.test.tsx` |
| `pages/apps/gateways/panels/GatewayAdvancedPanel.tsx` | `pages/secrets/MissingUserSecretsBanner.tsx` |
| `pages/apps/gateways/panels/OverviewPanel.tsx` | `pages/secrets/MyUserSecretsTab.tsx` |
| `pages/apps/gateways/panels/TokensPanel.test.tsx` | `pages/secrets/SetMyUserSecretDialog.tsx` |
| `pages/apps/gateways/panels/TokensPanel.tsx` | `pages/secrets/UserSecretDefinitionsTab.tsx` |
| `pages/apps/google-sheets.ts` | `pages/secrets/my-value-state.ts` |
| `pages/apps/store-cards.tsx` | `pages/secrets/user-secret-presentation.test.ts` |
| `pages/apps/useReviewCount.ts` | `pages/secrets/user-secret-presentation.tsx` |
| `pages/audit/AuditFeed.test.tsx` | `pages/tools/AdvancedToolsRoute.tsx` |
| `pages/audit/AuditFeed.tsx` | `pages/tools/AuditTab.test.tsx` |
| `pages/audit/CompanyActivity.tsx` | `pages/tools/AuditTab.tsx` |
| `pages/secrets/ImportFromVaultDialog.test.tsx` | `pages/tools/GatewaysTab.test.tsx` |
| `pages/secrets/ImportFromVaultDialog.tsx` | `pages/tools/GatewaysTab.tsx` |
| `pages/secrets/MissingUserSecretsBanner.test.tsx` | `pages/tools/PasteConfigTab.test.tsx` |
| `pages/secrets/MissingUserSecretsBanner.tsx` | `pages/tools/PasteConfigTab.tsx` |
| `pages/secrets/MyUserSecretsTab.tsx` | `pages/tools/PoliciesTab.test.tsx` |
| `pages/secrets/ProposalsTab.render.test.tsx` | `pages/tools/PoliciesTab.tsx` |
| `pages/secrets/ProposalsTab.tsx` | `pages/tools/ProfilesTab.test.ts` |
| `pages/secrets/SecretPathName.tsx` | `pages/tools/ProfilesTab.tsx` |
| `pages/secrets/SetMyUserSecretDialog.tsx` | `pages/tools/RunYourOwnTab.tsx` |
| `pages/secrets/UserSecretDefinitionsTab.tsx` | `pages/tools/RuntimeTab.test.tsx` |
| `pages/secrets/my-value-state.ts` | `pages/tools/RuntimeTab.tsx` |
| `pages/secrets/proposal-review.tsx` | `pages/tools/SmokeLabTab.test.tsx` |
| `pages/secrets/secret-path.test.ts` | `pages/tools/SmokeLabTab.tsx` |
| `pages/secrets/secret-path.ts` | `pages/tools/ToolsAccess.test.tsx` |
| `pages/secrets/user-secret-presentation.test.ts` | `pages/tools/ToolsAccess.tsx` |
| `pages/secrets/user-secret-presentation.tsx` | `pages/tools/connection-dialogs.tsx` |
| `pages/skills/ImportSkillsFromProjectDialog.test.tsx` | `pages/tools/profiles/ProfileActionDialog.tsx` |
| `pages/skills/ImportSkillsFromProjectDialog.tsx` | `pages/tools/profiles/ProfileDetail.test.tsx` |
| `pages/tools/AdvancedToolsRoute.tsx` | `pages/tools/profiles/ProfileDetail.tsx` |
| `pages/tools/AuditTab.test.tsx` | `pages/tools/profiles/ProfileDetailRoute.tsx` |
| `pages/tools/AuditTab.tsx` | `pages/tools/profiles/ProfileWizard.test.tsx` |
| `pages/tools/GatewaysTab.test.tsx` | `pages/tools/profiles/ProfileWizard.tsx` |
| `pages/tools/GatewaysTab.tsx` | `pages/tools/profiles/ProfileWizardRoute.tsx` |
| `pages/tools/PasteConfigTab.test.tsx` | `pages/tools/profiles/ProfilesIndex.test.tsx` |
| `pages/tools/PasteConfigTab.tsx` | `pages/tools/profiles/ProfilesIndex.tsx` |
| `pages/tools/PoliciesTab.test.tsx` | `pages/tools/profiles/ToolsAdminGate.tsx` |
| `pages/tools/PoliciesTab.tsx` | `pages/tools/profiles/WizardToolsStep.test.tsx` |
| `pages/tools/ProfilesTab.test.ts` | `pages/tools/profiles/WizardToolsStep.tsx` |
| `pages/tools/ProfilesTab.tsx` | `pages/tools/profiles/profile-model.test.ts` |
| `pages/tools/RunYourOwnTab.tsx` | `pages/tools/profiles/profile-model.ts` |
| `pages/tools/RuntimeTab.test.tsx` | `pages/tools/profiles/profile-summary.test.ts` |
| `pages/tools/RuntimeTab.tsx` | `pages/tools/profiles/profile-summary.ts` |
| `pages/tools/SmokeLabTab.test.tsx` | `pages/tools/profiles/useProfilesData.ts` |
| `pages/tools/SmokeLabTab.tsx` | `pages/tools/profiles/wizard-draft.test.ts` |
| `pages/tools/ToolsAccess.test.tsx` | `pages/tools/profiles/wizard-draft.ts` |
| `pages/tools/ToolsAccess.tsx` | `pages/tools/shared.tsx` |
| `pages/tools/connection-dialogs.tsx` | `pages/tools/smoke-lab-matrix.test.ts` |
| `pages/tools/profiles/ProfileActionDialog.tsx` | `pages/tools/smoke-lab-matrix.ts` |
| `pages/tools/profiles/ProfileDetail.test.tsx` | `pages/tools/tool-tabs.ts` |
| `pages/tools/profiles/ProfileDetail.tsx` | `pages/useInstallTeamCatalogEntry.test.tsx` |
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
- Parrot: **19**

| Paperclip evidence | Parrot evidence |
|---|---|
| `__tests__/decision-queues-routes.test.ts` | `heartbeat_service.rs` |
| `__tests__/heartbeat-accepted-plan-workspace-refresh.test.ts` | `job_scheduler.rs` |
| `__tests__/heartbeat-active-run-output-watchdog.test.ts` | `monitor_scheduler.rs` |
| `__tests__/heartbeat-agent-session-message.test.ts` | `plugin_job_coordinator.rs` |
| `__tests__/heartbeat-archived-company-guard.test.ts` | `plugin_job_scheduler.rs` |
| `__tests__/heartbeat-auto-checkout.test.ts` | `plugin_worker_manager.rs` |
| `__tests__/heartbeat-comment-wake-batching.test.ts` | `recovery_action_service.rs` |
| `__tests__/heartbeat-context-summary.test.ts` | `recovery_observability_service.rs` |
| `__tests__/heartbeat-cost-accounting.test.ts` | `routine_annotation_service.rs` |
| `__tests__/heartbeat-dependency-scheduling.test.ts` | `routine_coordinator_service.rs` |
| `__tests__/heartbeat-issue-liveness-escalation.test.ts` | `routine_execution_service.rs` |
| `__tests__/heartbeat-issue-rewake-throttle.test.ts` | `routine_service.rs` |
| `__tests__/heartbeat-ledger-billing-code.test.ts` | `routine_service_impl.rs` |
| `__tests__/heartbeat-list.test.ts` | `routine_template.rs` |
| `__tests__/heartbeat-local-environment.test.ts` | `routine_trigger_service.rs` |
| `__tests__/heartbeat-lock-release-on-reassignment.test.ts` | `routine_variable_service.rs` |
| `__tests__/heartbeat-managed-clone-credentials.test.ts` | `sagas/routine_trigger_saga.rs` |
| `__tests__/heartbeat-model-profile.test.ts` | `status_card_worker.rs` |
| `__tests__/heartbeat-pending-cleanup-sweep.test.ts` | `summary_slot_worker.rs` |
| `__tests__/heartbeat-plugin-environment.test.ts` | *(no structural counterpart in this slice)* |
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

- Paperclip: **475**
- Parrot: **51**

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
| `adapter-utils/src/acpx-engine/session-reuse-store.test.ts` | `repositories/src/secret_provider_config_repository.rs` |
| `adapter-utils/src/acpx-engine/session-reuse-store.ts` | `repositories/src/secret_repository.rs` |
| `adapter-utils/src/acpx-engine/settlement-characterization.test.ts` | `repositories/src/user_secret_repository.rs` |
| `adapter-utils/src/acpx-engine/settlement-sequence.test.ts` | `services/src/adapter_config_normalizer.rs` |
| `adapter-utils/src/acpx-engine/settlement-sequence.ts` | `services/src/adapter_executor.rs` |
| `adapter-utils/src/acpx-engine/spawn-smoke.test.ts` | `services/src/adapter_install_lock.rs` |
| `adapter-utils/src/acpx-engine/startup-characterization.test.ts` | `services/src/adapter_install_transaction.rs` |
| `adapter-utils/src/acpx-engine/startup-timing.test.ts` | `services/src/adapter_package_loader.rs` |
| `adapter-utils/src/acpx-engine/startup-timing.ts` | `services/src/adapter_plugin.rs` |
| `adapter-utils/src/acpx-engine/turn-characterization.test.ts` | `services/src/adapter_plugin_store.rs` |
| `adapter-utils/src/acpx-engine/turn-sequence.test.ts` | `services/src/adapter_registry.rs` |
| `adapter-utils/src/acpx-engine/turn-sequence.ts` | `services/src/adapter_registry_state.rs` |
| `adapter-utils/src/acpx-engine/ui.ts` | `services/src/adapters/claude_local_adapter.rs` |
| `adapter-utils/src/billing.test.ts` | `services/src/adapters/codex_local_adapter.rs` |
| `adapter-utils/src/billing.ts` | `services/src/adapters/mod.rs` |
| `adapter-utils/src/command-managed-runtime.test.ts` | `services/src/adapters/process_adapter.rs` |
| `adapter-utils/src/command-managed-runtime.ts` | `services/src/agent_secret_bindings_service.rs` |
| `adapter-utils/src/command-redaction.test.ts` | `services/src/asset_storage.rs` |
| `adapter-utils/src/command-redaction.ts` | `services/src/builtin_adapter_types.rs` |
| `adapter-utils/src/env-bindings.test.ts` | `services/src/database_secret_service.rs` |
| `adapter-utils/src/env-bindings.ts` | `services/src/environment_driver/sandbox_driver.rs` |
| `adapter-utils/src/exclude-patterns.ts` | `services/src/github_external_object_provider_service.rs` |
| `adapter-utils/src/execution-target-sandbox.test.ts` | `services/src/plugin_runtime_sandbox.rs` |
| `adapter-utils/src/execution-target-stdin-race.test.ts` | `services/src/secret_provider.rs` |
| `adapter-utils/src/execution-target.test.ts` | `services/src/secret_provider_config_service.rs` |
| `adapter-utils/src/execution-target.ts` | `services/src/secret_provider_service.rs` |
| `adapter-utils/src/git-workspace-sync.test.ts` | `services/src/secret_remote_import_service.rs` |
| `adapter-utils/src/git-workspace-sync.ts` | `services/src/secret_service.rs` |
| `adapter-utils/src/index.ts` | `services/src/server_adapter.rs` |
| `adapter-utils/src/local-process-sandbox.test.ts` | `services/src/user_secret_definition_service.rs` |
| `adapter-utils/src/local-process-sandbox.ts` | `services/src/user_secret_service.rs` |
| `adapter-utils/src/log-redaction.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/mcp-isolation.integration.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/remote-execution-env.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/remote-managed-runtime.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/remote-managed-runtime.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/runtime-progress.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/runtime-progress.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-callback-bridge.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-callback-bridge.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-file-sync.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-install-command.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-install-command.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-managed-runtime.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-managed-runtime.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-run-log-stream.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/sandbox-shell.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/server-utils-env.test.ts` | *(no structural counterpart in this slice)* |
| `adapter-utils/src/server-utils.test.ts` | *(no structural counterpart in this slice)* |
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
