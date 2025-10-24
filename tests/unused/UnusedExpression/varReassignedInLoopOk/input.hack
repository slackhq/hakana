function foo(): void {
    $b = 'hello again';
    $c = 'hello a third time';
    
    while (rand(0, 1)) {
        bar($c);

        if (rand(0, 1)) {
            // this reassignment is ok as variable was not previously used in loop
            $b = 'goodbye again';
            bar($b);
        }

        if (rand(0, 1)) {
            // this reassignment is fine because we're not referencing it within this block
            $c = 'goodbye';
        }

        // this is ok
        bar($b);
        bar($c);
    }
}

function bar(string $_s): void {}

function getNextDate(): string {
    $first_day_of_next_month = '01-01';
    $quarterly_dates = vec['01-01', '04-01', '07-01', '10-01'];

    while (rand(0, 1)) {
        if (C\contains($quarterly_dates, $first_day_of_next_month)) {
            break;
        }
        // This is a self-referential assignment and should not error
        $first_day_of_next_month = bar($first_day_of_next_month);
    }

    return $first_day_of_next_month;
}

