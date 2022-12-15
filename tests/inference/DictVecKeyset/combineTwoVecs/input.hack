function foo((int, int) $s): void {
	$a = rand(0, 1) ? $s : vec[];
    $b = rand(0, 1) ? vec[] : $s;
    if ($a) {}
    if ($b) {}
}