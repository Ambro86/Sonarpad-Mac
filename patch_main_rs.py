import os
import re

dir_path = r"C:\rustnotepad\Sonarpad minimal\src"

def patch_main():
    main_path = os.path.join(dir_path, 'main.rs')
    if not os.path.exists(main_path):
        return
        
    with open(main_path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Find the TvChannelCategory array logic and replace it.
    # We want dynamic categories. But the UI might expect a fixed list or a dynamic list.
    # Usually in desktop UI they build the list dynamically or hardcode it.
    
    # In main.rs, there's `fn tv_channel_categories() -> &'static [tv::TvChannelCategory]`
    # We'll just change `category: tv::TvChannelCategory` in state to `category: String`.
    
    # 1. State definition
    content = content.replace("category: tv::TvChannelCategory,", "category: String,")
    content = content.replace("category: tv::TvChannelCategory::Other,", 'category: "Altri".to_string(),')
    content = content.replace("category: tv::TvChannelCategory::Rai", 'category: "Rai".to_string()')
    
    # 2. tv_channel_categories and tv_channel_category_label
    # Replace those functions with a function that extracts unique categories
    content = re.sub(r"fn tv_channel_categories\([^)]*\)\s*->[^\{]*\{[^}]*\}", "", content)
    content = re.sub(r"fn tv_channel_category_label\([^)]*\)\s*->[^\{]*\{[^}]*\}", "", content)
    
    # Where it's used:
    # let categories = tv_channel_categories();
    # Let's replace usages.
    # Wait, it's easier to dynamically collect categories from channels.
    content = content.replace("let categories = tv_channel_categories();", """
        let mut categories: Vec<String> = channels.iter().map(|c| c.category.clone()).collect();
        categories.sort();
        categories.dedup();
    """)
    content = content.replace("tv_channel_category_label(*category).to_string()", "category.clone()")
    content = content.replace("tv_channel_category_label(category).to_string()", "category.clone()")
    
    content = content.replace("(channel.category == category).then_some(index)", "(channel.category == category).then_some(index)")
    
    # If there is `.position(|category| *category == channel.category)`
    content = content.replace(".position(|category| *category == channel.category)", ".position(|category| category == &channel.category)")
    content = content.replace(".unwrap_or(tv::TvChannelCategory::Rai)", '.unwrap_or("Rai".to_string())')

    with open(main_path, 'w', encoding='utf-8') as f:
        f.write(content)
        
    print("Updated main.rs")

if __name__ == '__main__':
    patch_main()
