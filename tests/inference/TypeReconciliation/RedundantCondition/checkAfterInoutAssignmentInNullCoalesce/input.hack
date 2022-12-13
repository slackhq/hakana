function foo(): void {
	$err = null;
	bar(inout $err) ?? null;
    if ($err is nonnull) {
	    echo $err;
    }
}

function bar(inout ?string $err): ?string {
  return "a";
}