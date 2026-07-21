fn main() {
    for cmd in ["trash ./dist", "trash ~/.ssh", "trash .git", "rm ./dist"] {
        println!("{cmd:20} -> {:?}", kxen_tools::safety::evaluate_shell_command(cmd, "/Users/test/project"));
    }
}
