function foo(dict<string, string> $dict): void {
}

foo(dict['b' => json_encode(dict[])]);