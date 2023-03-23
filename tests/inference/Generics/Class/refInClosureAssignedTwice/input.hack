function foo(string $current, string $path, vec<string> $arr): bool {
	$pattern_path = new HH\Lib\Ref(Vec\concat(vec[''], Str\split($path, ',')));

	$ret = new HH\Lib\Ref(null);
    Vec\map($arr, ($stack_item): void ==> {
		if ($ret->value is nonnull) return;

		$head_of_pattern = C\first($pattern_path->value);
		if ($head_of_pattern is null) {
			$ret->value = true;
			return;
		}

        /* HAKANA_FIXME[MixedAnyArgument] */
		if (Str\trim($head_of_pattern) !== Str\trim($stack_item)) {
			$ret->value = false;
			return;
		}

		$pattern_path->value = Vec\drop($pattern_path->value, 1);
	});

	if ($ret->value is nonnull) return $ret->value;

	$current_path = C\first($pattern_path->value);
	if ($current_path is nonnull) {
        /* HAKANA_FIXME[MixedAnyArgument] */
		if ($current_path != "" && Str\trim($current_path) !== Str\trim($current)) {
            return false;
        }
    }

	return true;
}