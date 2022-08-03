function foo(int $i, dict<int, dict<string, string>> $tokens) : string {
    if (isset($tokens[$i]["a"])) {
        return "hello";
    } else {
        return $tokens[$i]["b"];
    }
}