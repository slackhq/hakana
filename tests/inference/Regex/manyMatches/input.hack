use namespace HH\Lib\{Regex};

function foo(string $s): ?string {
    $pattern =
			re"/(?<![^\x{000A}\x{000D}\x{0009}\x{000C}\x{000B}\x{0020}\x{0022}\x{0027}\x{0060}\x{00AB}\x{201C}\x{0028}])[_A-Za-z0-9-\+]+(\.[_A-Za-z0-9-\+]+)*@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.([A-Za-z]{2,})\b/u";

    $matches = Regex\first_match(
		$s,
		$pattern,
	);

    if ($matches is null) {
        return null;
    }

    return $matches[0];
}

