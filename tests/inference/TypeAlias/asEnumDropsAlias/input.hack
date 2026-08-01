enum CalendarType: string {
	GOOGLE = 'google';
}

// `as SomeEnum` refines an aliased scalar to the enum itself.
function from_type(my_interop_string $t): CalendarType {
	return $t as CalendarType;
}
