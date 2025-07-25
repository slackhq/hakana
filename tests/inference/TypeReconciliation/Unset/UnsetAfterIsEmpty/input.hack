function foo(dict<string, string> $bar, string $baz): void {
    if (!C\is_empty($bar)) {
    	if (C\contains_key($bar, $baz)) {
        	unset($bar[$baz]);
        }
        
        if (!C\is_empty($bar)) {}
    }
}
