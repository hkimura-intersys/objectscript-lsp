Class MyApp.Child Extends Parent
{

/// Override of MyApp.Parent:Hello()
ClassMethod Hello() As %String
{
    Quit "Hello from Child (override)"
}

ClassMethod OnlyInChild() As %String
{
    Quit "This exists only on Child"
}

}
