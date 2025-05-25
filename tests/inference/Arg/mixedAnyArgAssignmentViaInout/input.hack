function foo(string $url): bool {
	$matches = dict[];
	\bar(inout $matches);
	if ($matches) {
		return C\count($matches) > 0;
	}
	return false;
}

function bar(inout HH\FIXME\MISSING_PARAM_TYPE $matches)[]: HH\FIXME\MISSING_RETURN_TYPE {}