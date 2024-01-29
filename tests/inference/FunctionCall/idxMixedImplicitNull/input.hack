function foo(dict<string, mixed> $args): void {
    $b = idx($args, 'b');
    if ($b === null) {}
}