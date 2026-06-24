function foo(): ?string {
  if (rand(0, 1) !== 0) {
    return;
  }

  return "hello";
}
