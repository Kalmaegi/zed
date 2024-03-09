use crate::{
    motion::Motion,
    state::{Mode, Operator},
    Vim,
};
use collections::HashMap;
use editor::{movement, Bias, DisplayPoint};
use gpui::{actions, ViewContext, WindowContext};
use log::error;
use std::ops::Range;
use std::sync::Arc;
use workspace::Workspace;

actions!(vim, [ToggleReplace]);

pub fn register(workspace: &mut Workspace, _: &mut ViewContext<Workspace>) {
    workspace.register_action(|_, _: &ToggleReplace, cx: &mut ViewContext<Workspace>| {
        Vim::update(cx, |vim, cx| {
            if vim.state().mode == Mode::Replace {
                vim.switch_mode(Mode::Normal, false, cx);
            } else {
                vim.switch_mode(Mode::Replace, false, cx);
                vim.update_active_editor(cx, |_, editor, cx| {
                    editor.set_last_snapshot(Some(editor.buffer().clone().read(cx).snapshot(cx)));
                    editor.vim_replace_map = Default::default();
                    editor.vim_replace_stack = Default::default();
                });
            }
        });
    });
}

pub fn replace_motion(
    motion: Motion,
    operator: Option<Operator>,
    times: Option<usize>,
    cx: &mut WindowContext,
) {
    Vim::update(cx, |vim, cx| {
        match operator {
            None => undo_replace_motion(vim, motion, times, cx),
            Some(operator) => {
                // Can't do anything for text objects, Ignoring
                error!("Unexpected replace mode motion operator: {:?}", operator)
            }
        }
    });
}

pub(crate) fn multi_replace(text: Arc<str>, cx: &mut WindowContext) {
    Vim::update(cx, |vim, cx| {
        // vim.stop_recording();
        vim.update_active_editor(cx, |_, editor, cx| {
            editor.transact(cx, |editor, cx| {
                editor.set_clip_at_line_ends(false, cx);
                let (map, display_selections) = editor.selections.all_display(cx);
                println!("map is displai is {:?}", display_selections);
                let edits = display_selections
                    .into_iter()
                    // .rev()
                    .map(|selection| {
                        println!(
                            "selection is {:?}, text is {:?}",
                            selection.id,
                            text.as_ref()
                        );
                        let is_new_line = text.as_ref() == "\n";
                        let mut range = selection.range();
                        // "\n" need to be handled separately, because when a "\n" is typing,
                        // we don't do a replace, we need insert a "\n"
                        if !is_new_line {
                            *range.end.column_mut() += 1;
                            range.end = map.clip_point(range.end, Bias::Right);
                        }
                        let replace_range = range.start.to_offset(&map, Bias::Left)
                            ..range.end.to_offset(&map, Bias::Right);
                        let snapshot = editor.buffer().read(cx).snapshot(cx);
                        println!(
                            "当前行字符长度{:?}, start {:?}, end {:?}",
                            snapshot.line_len(range.start.row()),
                            range.start.column(),
                            range.end.column()
                        );

                        let current_text = if is_new_line
                            || snapshot.line_len(range.start.row()) <= range.start.column()
                        {
                            "".to_string()
                        } else {
                            editor
                                .buffer()
                                .read(cx)
                                .snapshot(cx)
                                .chars_at(range.start.to_offset(&map, Bias::Left))
                                .next()
                                .map(|item| item.to_string())
                                .unwrap_or("".to_string())
                        };
                        println!(
                            "will replace {:?} to {:?}，target range is {:?}",
                            current_text,
                            text.as_ref(),
                            replace_range
                        );
                        // if replace_range.start == replace_range.end {
                        //     let vim_replace_map: HashMap<usize, String> = editor
                        //         .vim_replace_map
                        //         .iter()
                        //         .map(|(&offset, content)| {
                        //             if offset > replace_range.start {
                        //                 (offset + 1, content.clone())
                        //             } else {
                        //                 (offset, content.clone())
                        //             }
                        //         })
                        //         .collect();
                        //     // println!("replace new is {:?}", vim_replace_map);
                        //     editor.vim_replace_map = vim_replace_map;
                        // }
                        // 判断当前replace_range是否在vim_replace_map中，如果在，就跳过，如果不在，就插入
                        if !editor.vim_replace_map.contains_key(&replace_range) {
                            editor
                                .vim_replace_map
                                .insert(replace_range.clone(), current_text);
                        }

                        // println!("vim replace map is {:?}", editor.vim_replace_map);
                        // println!(
                        //     "will replace range {:?}, text {:?}",
                        //     replace_range.clone(),
                        //     text.clone()
                        // );
                        // println!("push after stack is {:?}", editor.vim_replace_stack);
                        (replace_range, text.clone())
                    })
                    .collect::<Vec<_>>();

                // Vec<Range<usize>, Arc<str, Global>>
                let stable_anchors = editor
                    .selections
                    .disjoint_anchors()
                    .into_iter()
                    // .rev()
                    .map(|selection| {
                        // println!(
                        //     "will selection replace range is {:?}",
                        //     selection.start.bias_right(&map.buffer_snapshot)
                        //         ..selection.start.bias_right(&map.buffer_snapshot)
                        // );
                        let start = selection.start.bias_right(&map.buffer_snapshot);
                        println!("current location is {:?}", selection.start);
                        println!("new location is {:?}", start);
                        start..start
                    })
                    .collect::<Vec<_>>();

                for edit in edits.iter().rev() {
                    // println!("will replace edit is {:?}", edit);
                    let (replace_range, _) = &edit;
                    if replace_range.start == replace_range.end {
                        let vim_replace_map: HashMap<Range<usize>, String> = editor
                            .vim_replace_map
                            .iter()
                            .map(|(range, content)| {
                                if range == replace_range {
                                    (range.start..range.end + 1, content.clone())
                                } else if range.start >= replace_range.start {
                                    (range.start + 1..range.end + 1, content.clone())
                                    // (range.clone(), content.clone())
                                } else {
                                    (range.clone(), content.clone())
                                }
                            })
                            .collect();
                        // println!("replace new is {:?}", vim_replace_map);
                        editor.vim_replace_map = vim_replace_map;
                    }
                }
                //
                // let edits = replaces
                //     .iter()
                //     .map(|(range, (from, to))| (range.clone(), to.clone()))
                //     .collect::<Vec<Range<usize>, Arc<str>>>();
                // println!("will replace edits {:?}", edits);

                editor.buffer().update(cx, |buffer, cx| {
                    println!("will replace edits {:?}", edits);
                    //需要处理每一次如果有换行，让后边的edit的range向后一位，否则会出现替换位移不对的问题
                    // 也可以提前遍历edit，然后将insert的单独处理
                    // for edit in edits {
                    //     buffer.edit([edit], None, cx);
                    // }
                    buffer.edit(edits, None, cx);
                });
                editor.set_clip_at_line_ends(true, cx);
                // for stable in stable_anchors {
                //     println!("will select is {:?}", stable);
                // }

                editor.change_selections(None, cx, |s| {
                    println!("will select is {:?}", stable_anchors);
                    s.select_anchor_ranges(stable_anchors);
                });
            });
        });
    });
}

fn undo_replace_motion(vim: &mut Vim, _: Motion, _: Option<usize>, cx: &mut WindowContext) {
    vim.stop_recording();
    vim.update_active_editor(cx, |_, editor, cx| {
        if let Some(original_snapshot) = editor.last_snapshot.clone() {
            editor.transact(cx, |editor, cx| {
                editor.set_clip_at_line_ends(false, cx);

                // for (map, display_selections) in editor.selections.all_display(cx) {
                //     println!("will redo, display is {:?}", display_selections);
                // }

                let (map, display_selections) = editor.selections.all_display(cx);
                println!("before undo selection is {:?}", display_selections);

                let (map1, adjust) = editor.selections.all_adjusted_display(cx);
                println!("before undo adjust is {:?}", adjust);
                let edits = display_selections
                    .into_iter()
                    .map(|selection| {
                        let snapshot = editor.buffer().read(cx).snapshot(cx);

                        let mut range = selection.range();
                        println!("before range is {:?}", range);
                        if range.start.column() > 0 {
                            *range.start.column_mut() -= 1;
                        } else if range.start.row() > 0 {
                            *range.start.row_mut() -= 1;
                            *range.start.column_mut() = snapshot.line_len(range.start.row()) + 1;
                        } else {
                            *range.end.column_mut() += 1;
                        }
                        range.start = map.clip_point(range.start, Bias::Left);
                        range.end = map.clip_point(range.end, Bias::Left);
                        println!("final range is {:?}", range);
                        let cur_range = range.start.to_offset(&map, Bias::Left)
                            ..range.end.to_offset(&map, Bias::Left);
                        println!("undo target range is {:?}", cur_range);
                        // println!(
                        //     "replace undo current stack is {:?}",
                        //     editor.vim_replace_stack
                        // );
                        println!("undo replace map is {:?}", editor.vim_replace_map);

                        // let (repalce_range, content) = editor.vim_replace_stack
                        let mut replace_text = editor
                            .buffer()
                            .read(cx)
                            .snapshot(cx)
                            .chars_at(range.start.to_offset(&map, Bias::Left))
                            .next()
                            .map(|item| item.to_string())
                            .unwrap_or("".to_string());

                        // if let Some(last) = editor.vim_replace_stack.last() {
                        //     println!("stack last item is {:?}", last);
                        //     let (replace_range, content) = last;

                        //     println!(
                        //         "cur range {:?}. {:?}, stack range {:?}, {:?}",
                        //         cur_range.start,
                        //         cur_range.end,
                        //         replace_range.start,
                        //         replace_range.end
                        //     );

                        //     if range.start.to_offset(&map, Bias::Left) == replace_range.start {
                        //         replace_text = content.to_string();
                        //         println!("replace text is {:?}", replace_text);
                        //         editor.vim_replace_stack.pop();
                        //     }
                        // }
                        //

                        if let Some(last) = editor.vim_replace_map.get(&cur_range) {
                            replace_text = last.to_string();
                            editor.vim_replace_map.remove(&cur_range);
                        } else {
                            println!("某些字符没取到{:?}", cur_range);
                        }

                        let select = if replace_text == "" {
                            selection.range()
                        } else {
                            println!("will redo range is {:?}", range);
                            range.clone()
                        };
                        println!("select is {:?}", select);
                        // if replace_text == "" {
                        //     let vim_replace_map: HashMap<Range<usize>, String> = editor
                        //         .vim_replace_map
                        //         .iter()
                        //         .map(|(range, content)| {
                        //             if range.start >= cur_range.start {
                        //                 (range.start - 1..range.end - 1, content.clone())
                        //             } else {
                        //                 (range.clone(), content.clone())
                        //             }
                        //         })
                        //         .collect();
                        //     println!("undo replace new is {:?}", vim_replace_map);
                        //     editor.vim_replace_map = vim_replace_map;
                        // }

                        println!(
                            "undo replace range is {:?}, text is {:?}",
                            cur_range, replace_text
                        );
                        (select, (cur_range, replace_text))
                    })
                    .collect::<Vec<_>>();

                // let (map, display_selections) = editor.selections.all_display(cx);
                // let stable_anchors = display_selections
                //     .into_iter()
                //     .map(|selection| {
                //         println!("undo select is {:?}", selection);
                //         let range = movement::left(&map, selection.start)
                //             ..movement::left(&map, selection.start);
                //         range
                //     })
                //     .collect::<Vec<_>>();
                // println!("undo stable is {:?}", stable_anchors);
                // println!("undo before replace new is {:?}", editor.vim_replace_map);

                // for edit in edits.iter() {
                //     // println!("undo replace edit is {:?}", edit);
                //     let (edit_range, content) = edit;
                //     if content == "" {
                //         let vim_replace_map: HashMap<Range<usize>, String> = editor
                //             .vim_replace_map
                //             .iter()
                //             .map(|(range, content)| {
                //                 if range.start > edit_range.start {
                //                     (range.start - 1..range.end - 1, content.clone())
                //                 } else {
                //                     (range.clone(), content.clone())
                //                 }
                //             })
                //             .collect();
                //         // println!("undo replace new is {:?}", vim_replace_map);
                //         editor.vim_replace_map = vim_replace_map;
                //     }
                // }

                // let stable_anchors = edits
                //     .iter()
                //     .map(|edit| {
                //         let (edit_range, content) = edit;
                //         // let range = movement::left(&map, edit_range.start)
                //         //     ..movement::left(&map, edit_range.start);
                //         if content == "" {
                //             DisplayPoint::new(edit_range.start as u32, edit_range.end as u32)
                //                 ..DisplayPoint::new(edit_range.start as u32, edit_range.end as u32)
                //         } else {
                //             DisplayPoint::new(edit_range.start as u32, edit_range.end as u32)
                //                 ..DisplayPoint::new(edit_range.start as u32, edit_range.end as u32)
                //         }
                //     })
                //     .collect::<Vec<_>>();
                //将edits变量解包
                let (anchors, will_edits): (Vec<_>, Vec<_>) = edits.into_iter().unzip();
                println!("aanchor is {:?}", anchors);

                //创建一个vec，其中的内容是edits的倒序
                let mut rel = will_edits;
                let re: Vec<(Range<usize>, String)> = rel.iter().rev().cloned().collect();

                // let mut rel = edits;
                // let re: Vec<(Range<usize>, String)> = rel.reverse();
                // println!("rel is {:?}", rel);
                editor.buffer().update(cx, |buffer, i_cx| {
                    println!("final edit is {:?}", re);
                    for edit in re {
                        buffer.edit([edit.clone()], None, i_cx);
                        // let (edit_range, content) = edit;
                        // println!("will edit {:?}", edit);
                        // if content != "" {
                        //     let target =
                        //         DisplayPoint::new(edit_range.start as u32, edit_range.end as u32)
                        //             ..DisplayPoint::new(
                        //                 edit_range.start as u32,
                        //                 edit_range.end as u32,
                        //             );
                        //     editor.change_selections(None, cx, |s| {
                        //         // s.select_ranges(target);
                        //         s.select_display_ranges(target);
                        //     });
                        // };
                    }
                });

                // for stable_anchor in stable_anchors {
                //     println!("undo stable is {:?}", stable_anchor);
                //     let (map, display_selections) = editor.selections.all_display(cx);
                //     println!("display selection is {:?}", display_selections);
                //     editor.change_selections(None, cx, |s| {
                //         s.select_display_ranges([stable_anchor]);
                //     });
                // }
                //
                editor.change_selections(None, cx, |s| {
                    s.select_display_ranges(anchors);
                });
                editor.set_clip_at_line_ends(true, cx);
            });
        };
    });
}
