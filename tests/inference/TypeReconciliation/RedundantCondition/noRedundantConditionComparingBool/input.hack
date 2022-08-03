function getBool(): bool {
  return (bool)rand(0, 1);
}

function takesBool(bool $b): void {
  if ($b === getBool()) {}
}