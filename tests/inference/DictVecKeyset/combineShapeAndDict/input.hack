function foo(dict<string, mixed> $dict): ?int {
  if (rand(0, 1)) {
    $dict = dict['foo' => 5];
  }
  return $dict['foo'] ?? null;
}

function bar(shape('foo' => int) $dict, dict<string, mixed> $dict2): ?int {
  if (rand(0, 1)) {
    $dict = $dict2;
  }
  return $dict['foo'] ?? null;
}