function foo(vec<int> $arr): dict<int, shape('a' => vec<int>)> {
  $foo = dict[];
  
  foreach ($arr as $v) {
    if (rand(0, 1)) {
      $foo[$v] = shape(
        'a' => $foo[$v]['a'] ?? vec[]
      );
    }
    
    if (rand(0, 1)) {
      $foo[$v]['a'][] = 5;
    }
  }
  
  return $foo;
}