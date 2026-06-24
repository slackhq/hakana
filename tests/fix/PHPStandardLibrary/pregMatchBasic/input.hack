function takes_string(string $haystack): void {
    preg_match("/^foo$/", $haystack);
    preg_match('/^foo$/', $haystack);
}
