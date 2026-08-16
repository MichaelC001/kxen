// safety 命令评估与路径守卫测试。
use kxen_core::tools::safety::{Verdict, evaluate_shell_command, guard_path};

const CWD: &str = "/Users/test/project";

fn denied(cmd: &str) -> bool {
    matches!(evaluate_shell_command(cmd, CWD), Verdict::Deny { .. })
}

fn allowed(cmd: &str) -> bool {
    matches!(evaluate_shell_command(cmd, CWD), Verdict::Allow | Verdict::Recoverable)
}

#[test]
fn f1_system() {
    for cmd in [
        "rm -rf /",
        "rm -rf /usr",
        "sudo rm -rf /etc",
        "dd if=/dev/zero of=/dev/disk0",
        "mkfs.ext4 /dev/sda1",
        "diskutil eraseDisk JHFS+ New disk0",
        "find / -name x -delete",
    ] {
        assert!(denied(cmd), "should deny: {cmd}");
    }
}

#[test]
fn macos_temp_exempt() {
    assert!(denied("rm -rf /private/var/folders/qb/xxx/T/test"));
    assert!(denied("rm -rf /private/tmp/foo"));
    assert!(denied("rm -rf /tmp/foo"));
    assert!(allowed("trash /private/tmp/foo"));
    assert!(denied("rm -rf /private/etc"));
    assert!(denied("rm -rf /private/var/db"));
}

#[test]
fn separators_and_substitutions() {
    // || 与换行同样切段
    assert!(denied("ls || rm -rf /private/etc"));
    assert!(denied("ls\nrm -rf /private/etc"));
    // 反引号 / $() 内嵌命令纳入评估
    assert!(denied("echo $(rm -rf /private/etc)"));
    assert!(denied("echo `rm -rf /private/etc`"));
    assert!(allowed("echo $(ls -la)"));
}

#[test]
fn ask_verdict() {
    assert!(matches!(evaluate_shell_command("git push --force origin main", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git push -f origin main", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git reset --hard", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("sudo apt install jq", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git clean -fd", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("kill -9 1234", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("brew uninstall node", CWD), Verdict::Ask { .. }));
    // 带 ref 的 reset --hard 同样丢弃未提交改动，升 Ask
    assert!(matches!(evaluate_shell_command("git reset --hard HEAD", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git reset --hard origin/main", CWD), Verdict::Ask { .. }));
    // 具体危险优先于审批：sudo rm -rf /etc 仍是 Deny 不是 Ask
    assert!(matches!(evaluate_shell_command("sudo rm -rf /etc", CWD), Verdict::Deny { .. }));
}

#[test]
fn ask_interpreter_inline_scripts() {
    // 解释器内联脚本是 opaque token，脱离切段评估：统一升 Ask
    for cmd in [
        "python3 -c \"print(1)\"",
        "python -c \"print(1)\"",
        "node -e \"console.log(1)\"",
        "node --eval \"console.log(1)\"",
        "perl -e \"print 1\"",
        "ruby -e \"puts 1\"",
        "osascript -e \"tell application \\\"Finder\\\" to activate\"",
    ] {
        assert!(matches!(evaluate_shell_command(cmd, CWD), Verdict::Ask { .. }), "should ask: {cmd}");
    }
    // 脚本文件执行与无内联标志的调用不升档
    assert!(allowed("python3 script.py"));
    assert!(allowed("node build.mjs"));
    // 内联脚本里的具体危险仍是 Deny 优先
    assert!(denied("python3 -c \"mkfs.ext4 /dev/sda1\""));
}

#[test]
fn ask_process_substitution() {
    assert!(matches!(evaluate_shell_command("diff <(ls a) <(ls b)", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("cat <(echo hi)", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("tee >(gzip > out.gz)", CWD), Verdict::Ask { .. }));
    // 引号内的 <( 是字面文本
    assert!(allowed("echo \"<(not real)\""));
    // 进程替换内嵌的具体危险仍是 Deny 优先
    assert!(denied("cat <(rm -rf /private/etc)"));
}

#[test]
fn ask_git_worktree_destructive() {
    assert!(matches!(evaluate_shell_command("git checkout -- src/main.rs", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git checkout -- .", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git checkout .", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git checkout ./src", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git restore src/main.rs", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git restore --worktree .", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git restore --source=HEAD~1 src", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git restore --staged --worktree src", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git stash drop", CWD), Verdict::Ask { .. }));
    assert!(matches!(evaluate_shell_command("git stash clear", CWD), Verdict::Ask { .. }));
    // 安全形态不升档：切分支 / 建分支 / 仅取消暂存
    assert!(allowed("git checkout main"));
    assert!(allowed("git checkout -b feature-x"));
    assert!(allowed("git restore --staged src/main.rs"));
    assert!(allowed("git stash pop"));
    assert!(allowed("git stash list"));
}

#[test]
fn f2_home() {
    assert!(denied("rm -rf ~"));
    assert!(denied("rm -rf ~/Documents"));
    assert!(denied("trash ~/.ssh"));
    assert!(denied("rm ~/Documents/draft.txt"));
    assert!(allowed("trash ~/Documents/draft.txt"));
}

#[test]
fn f3_git() {
    assert!(denied("rm -rf .git"));
    assert!(denied("mv .git /tmp/trash"));
    assert!(denied("git update-ref -d refs/heads/main"));
    assert!(matches!(evaluate_shell_command("git reset --hard HEAD", CWD), Verdict::Ask { .. }));
    assert!(allowed("git branch -d feature-x"));
}

#[test]
fn f4_destroy() {
    for cmd in
        ["terraform destroy", "dropdb production", "kubectl delete ns prod", "aws s3 rb s3://b --force", "docker system prune --volumes"]
    {
        assert!(denied(cmd), "should deny: {cmd}");
    }
}

#[test]
fn f5_bypass() {
    assert!(denied("bash -c \"rm -rf /usr\""));
    assert!(denied("rm -rf $DIR/"));
}

#[test]
fn permanent_delete_commands_are_rejected_in_every_spelling() {
    let cases = [
        "rm ./artifact.txt",
        "command rm -rf ./dist",
        "env MODE=ci /bin/rm ./artifact.txt",
        "env -S 'rm ./artifact.txt'",
        "/usr/bin/rmdir ./empty-dir",
        "/usr/bin/unlink ./artifact.txt",
        "find ./target -delete",
        "/usr/bin/find ./target -type f -delete",
        "find ./target -exec rm -f {} +",
        "find ./target -ok /bin/rm -f {} +",
        "printf '%s\\0' ./artifact.txt | xargs -0 rm -f",
        "xargs -a ./paths.txt /bin/rm -f",
        "bash -c 'rm -rf ./dist'",
        "bash -lc 'rm -rf ./dist'",
        "bash --norc -c 'rm -rf ./dist'",
        "sh -c '\"$@\"' _ rm ./artifact.txt",
        "/bin/sh -c \"command /bin/rm ./artifact.txt\"",
        "env -i zsh -c 'unlink ./artifact.txt'",
        "if test -e ./artifact.txt; then /bin/rm ./artifact.txt; fi",
        "(command rm ./artifact.txt)",
        "TARGET=./artifact.txt; CMD=rm; $CMD $TARGET",
        "eval 'rm ./artifact.txt'",
        "nohup rm ./artifact.txt",
        "nice -n 5 rm ./artifact.txt",
        "time -p rm ./artifact.txt",
        "timeout -k 1 5 rm ./artifact.txt",
        "setsid rm ./artifact.txt",
        "busybox rm ./artifact.txt",
        "exec /bin/rm ./artifact.txt",
        "sudo -u root /bin/rm ./artifact.txt",
        "printf 'rm ./artifact.txt\\n' | sh",
        "printf 'rm ./artifact.txt\\n' | dash",
        "source ./cleanup.sh",
        ". ./cleanup.sh",
    ];

    for command in cases {
        let verdict = evaluate_shell_command(command, CWD);
        match verdict {
            Verdict::Deny { reason, suggestion, .. } => {
                assert!(reason.contains("不可恢复"), "拒绝原因需说明风险: {command}: {reason}");
                assert!(suggestion.is_some_and(|value| value.contains("delete tool")), "须引导使用 delete tool: {command}");
            }
            other => panic!("不可恢复删除必须 fail closed: {command}: {other:?}"),
        }
    }
}

#[test]
fn non_executing_delete_mentions_remain_allowed() {
    for command in [
        "printf '%s\\n' 'rm ./artifact.txt'",
        "command -v rm",
        "find . -name delete",
        "printf '%s\\0' ./artifact.txt | xargs -0 echo rm",
        "bash -c 'printf %s rm'",
        "bash --version",
        "bash -n ./cleanup.sh",
    ] {
        assert!(allowed(command), "只把删除命令当数据使用时不应误拦: {command}");
    }
}

#[test]
fn env_assignment_prefix_does_not_bypass() {
    // 前导 VAR=value 赋值不是命令本身：跳过赋值后的真命令照样进删除判定
    assert!(denied("X=1 rm -rf ~"));
    assert!(denied("A=1 B=2 rm -rf /private/etc"));
    assert!(denied("sudo X=1 rm -rf /usr"));
    assert!(allowed("X=1 ls -la"));
}

#[test]
fn nested_substitutions_are_evaluated() {
    // 平衡括号：嵌套 $() 的内层命令同样进评估（非嵌套正则只捕到残缺外层）
    assert!(denied("echo $(cat $(rm -rf /private/etc))"));
    assert!(denied("echo $(ls $(rm -rf ~))"));
    assert!(allowed("echo $(ls $(pwd))"));
}

#[test]
fn f2_credential_list_matches_path_policy() {
    // 与 path_policy::sensitive_reason 同一清单：任一漏项都是删除凭证的绕过通道
    for dot in [".ssh", ".gnupg", ".aws", ".kube", ".docker", ".codex", ".claude", ".grok", ".kimi-code"] {
        assert!(denied(&format!("rm -rf ~/{dot}")), "~/{dot} 应被拒绝");
    }
}

#[test]
fn trash_recoverable() {
    assert!(matches!(evaluate_shell_command("trash ./dist", CWD), Verdict::Recoverable));
    assert!(matches!(evaluate_shell_command("find ./dist -exec trash {} +", CWD), Verdict::Recoverable));
    assert!(denied("trash .git"));
    assert!(denied("find .git -exec trash {} +"));
    assert!(denied("find \"$HOME/.ssh\" -exec trash {} +"));
    assert!(denied("trash ~/.ssh"));
}

#[test]
fn guard() {
    assert!(matches!(guard_path("~/.ssh/id_rsa", CWD), Verdict::Deny { .. }));
    assert!(matches!(guard_path(".git/config", CWD), Verdict::Deny { .. }));
    assert!(matches!(guard_path("src/index.ts", CWD), Verdict::Allow));
}
