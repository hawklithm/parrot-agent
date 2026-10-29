const CEO_TEMPLATE: &str = include_str!(
    "../../../teams-catalog/catalog/bundled/company-defaults/core-exec-team/agents/ceo/AGENTS.md"
);

fn section(heading: &str) -> &str {
    let start = CEO_TEMPLATE
        .find(heading)
        .unwrap_or_else(|| panic!("CEO template is missing section: {heading}"));
    let content = &CEO_TEMPLATE[start..];
    let next_section = content[heading.len()..]
        .find("\n## ")
        .map(|offset| heading.len() + offset);
    next_section.map_or(content, |end| &content[..end])
}

#[test]
fn team_shape_is_derived_per_task_without_fixed_roles_or_headcount() {
    let policy = section("## 按每个任务动态决定团队");

    for required in [
        "每项用户任务",
        "所需职责",
        "成员人数",
        "汇报层级",
        "任务分派",
        "验收方式",
        "零新增成员",
        "不预设 CTO、QA",
        "团队目录模板",
        "可选参考",
    ] {
        assert!(policy.contains(required), "team policy is missing: {required}");
    }

    assert!(!CEO_TEMPLATE.contains("技术实现通常交 CTO"));
    assert!(!CEO_TEMPLATE.contains("由 CTO 拆解并安排工程与 QA"));
}

#[test]
fn simple_work_can_stay_with_one_owner_without_splitting_or_hiring() {
    let policy = section("## 按每个任务动态决定团队");

    for required in [
        "简单、边界清楚",
        "无需拆分、建立额外层级或创建成员",
        "单一负责人",
        "CEO 处理",
        "最匹配且有容量的现有成员",
    ] {
        assert!(policy.contains(required), "simple-task policy is missing: {required}");
    }
}

#[test]
fn membership_changes_preserve_history_and_use_real_approved_tools() {
    let membership = section("## 复用团队与招聘 Agent");
    let boundaries = section("## 审批与操作边界");

    for required in [
        "paperclipListAgents",
        "paperclipHireAgent",
        "`reportsTo`",
        "退出本次任务团队",
        "不等于终止公司级 Agent",
        "保留任务、成果、汇报关系和变更的可追溯历史",
        "不得编造工具名、API、endpoint、参数",
    ] {
        assert!(membership.contains(required), "membership policy is missing: {required}");
    }

    for required in ["审批", "密钥", "凭据"] {
        assert!(
            membership.contains(required) || boundaries.contains(required),
            "operational boundary is missing: {required}"
        );
    }
}

#[test]
fn clarifications_assignments_and_acceptance_follow_the_selected_task_structure() {
    let assessment = section("## 任务评估与需求澄清");
    let composition = section("## 按每个任务动态决定团队");
    let reporting = section("## 汇报关系与进度管理");
    let acceptance = section("## 子任务审查与退回");

    assert!(assessment.contains("向提出任务的用户问最少且具体的问题"));
    assert!(assessment.contains("依赖未决答案的执行先停下"));
    assert!(composition.contains("单一负责人"));
    assert!(reporting.contains("按本任务实际设置的汇报线"));
    assert!(acceptance.contains("不默认创建 QA 或其他固定复核岗位"));
}
