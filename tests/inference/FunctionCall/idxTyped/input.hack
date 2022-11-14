function bar(dict<string, string> $args): string {
	$a = idx($args, 'a');
    if ($a === null) {
        return '';
    }
    return $a;
}