function foo(int $i, dict<int, dict<string, string>> $tokens) : void {
    if (!isset($tokens[$i]["a"])) {
        echo $tokens[$i]["b"];
    }
}