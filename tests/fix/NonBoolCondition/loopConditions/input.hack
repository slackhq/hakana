function main(int $i, ?bool $nullable): void {
    do {
        $i++;
    } while ($nullable);

    while ($i && $nullable) {
        break;
    }
}
