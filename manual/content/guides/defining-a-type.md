---
title: "Defining your own type"
description: "Build a small Pokémon card collection with fields you choose."
sidebar_position: 1
---

Start from a built-in type when it is close to what you need. Create your own when the collection needs different details. Here is a small **Cards** type for Pokémon cards.

## Create the type

Open the [schema editor](../features/schema.mdx) and add a type. Give it the label **Cards**, the ID `cards`, and the folder `Cards`.

Choose a title language for filenames. A name such as `Pikachu (My Collection).md` will then serve as the entity's title and link target.

## Add a few fields

Start with only the details you want to record:

| Field name | Field type | Use it for |
| --- | --- | --- |
| `photo` | Image | Your own photo of the card. |
| `set` | Text | The set it belongs to. |
| `condition` | Enum | Your own choices, such as Mint, Good, or Worn. |
| `franchise` | Relation | A link to a Franchise entity, such as Pokémon. |

For the relation field, choose **Franchise** as the target type. Add that type first if your vault does not have it.

Save the schema, then create an entity with [Manual Add](../features/adding.mdx#manual-add). Check that the photo and fields appear as you expected before adding the rest of your collection.

## Change it as you go

You can add fields later. Removing a field from the schema leaves its values in existing notes. If you rename a field, update the corresponding properties in your notes too.

Use [date and status roles](../reference/titles-dates-status.md) when you want calendar or progress behavior. Otherwise, a simple text field may be all you need. The [field reference](../reference/field-types.md) lists the available choices.
