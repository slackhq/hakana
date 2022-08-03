function bar(string $method): ?string {
	return HH\Lib\Str\split($method, '.')[0] ?? null;
}