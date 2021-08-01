ALTER TABLE meal
    ADD CONSTRAINT fk_meal_recipe
    FOREIGN KEY (recipe_uid)
    REFERENCES recipe (uid)
    DEFERRABLE INITIALLY IMMEDIATE,

    ADD CONSTRAINT fk_meal_type
    FOREIGN KEY (type_uid)
    REFERENCES meal_type (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE grocery_item
    ADD CONSTRAINT fk_grocery_item_recipe
    FOREIGN KEY (recipe_uid)
    REFERENCES recipe (uid)
    DEFERRABLE INITIALLY IMMEDIATE,

    ADD CONSTRAINT fk_grocery_item_aisle
    FOREIGN KEY (aisle_uid)
    REFERENCES aisle (uid)
    DEFERRABLE INITIALLY IMMEDIATE,

    ADD CONSTRAINT fk_grocery_item_list
    FOREIGN KEY (list_uid)
    REFERENCES grocery_list (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE menu_item
    ADD CONSTRAINT fk_menu_item_recipe
    FOREIGN KEY (recipe_uid)
    REFERENCES recipe (uid)
    DEFERRABLE INITIALLY IMMEDIATE,

    ADD CONSTRAINT fk_menu_item_menu
    FOREIGN KEY (menu_uid)
    REFERENCES menu (uid)
    DEFERRABLE INITIALLY IMMEDIATE,

    ADD CONSTRAINT fk_menu_item_type
    FOREIGN KEY (type_uid)
    REFERENCES meal_type (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE photo
    ADD CONSTRAINT fk_photo_recipe
    FOREIGN KEY (recipe_uid)
    REFERENCES recipe (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE pantry_item
    ADD CONSTRAINT fk_pantry_item_aisle
    FOREIGN KEY (aisle_uid)
    REFERENCES aisle (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE grocery_ingredient
    ADD CONSTRAINT fk_grocery_ingredient_aisle
    FOREIGN KEY (aisle_uid)
    REFERENCES aisle (uid)
    DEFERRABLE INITIALLY IMMEDIATE;

ALTER TABLE category
    ADD CONSTRAINT fk_category_parent
    FOREIGN KEY (parent_uid)
    REFERENCES category (uid)
    DEFERRABLE INITIALLY IMMEDIATE;
