function bar(dict<int, shape('a' => int)> $arr): void {
  $copy = dict[];
  foreach ($arr as $v) {
    $copy[$v['a']] ??= vec[];
    $copy[$v['a']][] = 'hello';
  }
  foreach ($copy as $v) {
    echo $v[0];
  }
}
