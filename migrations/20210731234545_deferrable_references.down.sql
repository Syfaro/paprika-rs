ALTER TABLE meal
    DROP CONSTRAINT fk_meal_recipe,
    DROP CONSTRAINT fk_meal_type;

ALTER TABLE grocery_item
    DROP CONSTRAINT fk_grocery_item_recipe,
    DROP CONSTRAINT fk_grocery_item_aisle,
    DROP CONSTRAINT fk_grocery_item_list;

ALTER TABLE menu_item
    DROP CONSTRAINT fk_menu_item_recipe,
    DROP CONSTRAINT fk_menu_item_menu,
    DROP CONSTRAINT fk_menu_item_type;

ALTER TABLE photo
    DROP CONSTRAINT fk_photo_recipe;

ALTER TABLE pantry_item
    DROP CONSTRAINT fk_pantry_item_aisle;

ALTER TABLE grocery_ingredient
    DROP CONSTRAINT fk_grocery_ingredient_aisle;

ALTER TABLE category
    DROP CONSTRAINT fk_category_parent;
