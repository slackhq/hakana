function takes_string(string $pattern, string $haystack, string $foo): void {
    preg_match($pattern, $haystack);
    preg_match("/test{$foo}/", $haystack);
}
