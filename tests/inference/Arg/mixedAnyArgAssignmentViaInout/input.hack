function foo(string $url): bool {
	$matches = dict[];
	\bar(inout $matches);
	return $matches && C\count($matches) > 0;
}

function bar(inout HH\FIXME\MISSING_PARAM_TYPE $matches)[]: HH\FIXME\MISSING_RETURN_TYPE {}