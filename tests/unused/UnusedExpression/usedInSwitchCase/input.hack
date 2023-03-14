function foo(string $s): void {
    $a = "a";
    $b = "b";

    switch ($s) {
      case $a:
        echo "cool";
        return;
      case $b:
      	echo "also cool";
        return;
    }
}