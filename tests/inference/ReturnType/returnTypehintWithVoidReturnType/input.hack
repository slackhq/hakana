function foo(): ?string {
  if (rand(0, 1)) {
    return;
  }

  return "hello";
}