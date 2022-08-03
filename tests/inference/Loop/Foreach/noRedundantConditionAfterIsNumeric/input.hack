$ids = vec[];
foreach (explode(",", "hello,5,20") as $i) {
  if (!is_numeric($i)) {
    continue;
  }

  $ids[] = $i;
}