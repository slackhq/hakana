type user = shape(
	'id' => int,
	'name' => string,
);

function takesUser(user $user): void {}

function takesDict(dict<string, mixed> $dict): void {
    takesUser($dict as user);
}