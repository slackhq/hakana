function getBool(): bool {
  return rand(0, 1) === 1;
}

function takesBool(bool $b): void {
  if ($b === getBool()) {}
}