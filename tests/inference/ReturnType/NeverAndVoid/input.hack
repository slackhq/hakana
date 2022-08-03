function foo(): void
{
    foreach (vec[0, 1, 2] as $_i) {
        return;
    }

    throw new \Exception();
}